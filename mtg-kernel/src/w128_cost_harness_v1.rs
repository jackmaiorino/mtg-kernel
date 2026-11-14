//! Timing-only harness (ignored tests): CPU cost of the default net
//! `kernel-policy-value-net-8` (w64: hidden 64, card embedding 16) against the
//! capacity-experiment wide row `kernel-policy-value-net-8w128` (w128: hidden
//! 128, card embedding 32), both loaded from their committed snapshots
//! (`data/common_model_snapshot_v1` and `data/wide_model_snapshot_w128`).
//!
//! - M1: CPU forward cost per decision on the real replay cases of
//!   `data/flat_policy_v2/python_full_features_v2.json` (the fixture the Burn
//!   packed-forward tests load), w64 and w128 in interleaved ABBA blocks.
//! - M2: per-update train-state digest, owned snapshot and train-state payload
//!   encode cost for both widths, plus the encoded byte counts.
//! - M3: one narrow CPU train step (forward with tape, backward, Adam) on the
//!   same cases, against a plain narrow forward of the same batch and a
//!   one-decision train step, so the per-decision backward cost can be
//!   separated from the fixed per-update cost. No wide backward exists, so
//!   there is no wide M3 row.
//!
//! Each test prints one JSON line to stdout. Measurement only: nothing here is
//! reachable from production code and no production path changes.
//!
//! Environment (all optional): `HARNESS_REPS` timed repetitions per arm
//! (default: sized from the warm-up to spend about `HARNESS_TARGET_SECONDS`,
//! default 40 for M1 and 30 for M2 and M3), `HARNESS_WARMUP` warm-up
//! repetitions per arm, `HARNESS_BLOCK` passes per ABBA block (M1 only).

use crate::common_model_snapshot_v1 as snapshot;
use crate::native_policy_train_step_v1 as train;
use crate::native_policy_value_net_v1 as net;
use crate::native_train_state_payload_v1 as payload;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::hint::black_box;
use std::time::Instant;

const FIXTURE_PATH: &str = "data/flat_policy_v2/python_full_features_v2.json";
const FIXTURE_BYTES: &[u8] =
    include_bytes!("../../data/flat_policy_v2/python_full_features_v2.json");
const VALUE_COEFFICIENT: f32 = 0.5;
const LEARNING_RATE: f32 = 0.000_01;

#[derive(Deserialize)]
struct FixtureDocumentV1 {
    cases: Vec<FixtureCaseV1>,
}

#[derive(Deserialize)]
struct FixtureCaseV1 {
    name: String,
    tensors: FixtureTensorsV1,
}

#[derive(Deserialize)]
struct FixtureTensorsV1 {
    state: F32TensorV1,
    object_features: F32TensorV1,
    object_card_ids: I64TensorV1,
    object_groups: I64TensorV1,
    object_node_ids: I64TensorV1,
    edge_features: F32TensorV1,
    edge_source_indices: I64TensorV1,
    edge_target_indices: I64TensorV1,
    action_features: F32TensorV1,
    action_ref_features: F32TensorV1,
    action_ref_card_ids: I64TensorV1,
    action_ref_action_indices: I64TensorV1,
    action_ref_node_indices: I64TensorV1,
}

#[derive(Deserialize)]
struct F32TensorV1 {
    f32_le_hex: String,
}

#[derive(Deserialize)]
struct I64TensorV1 {
    i64_values: Vec<i64>,
}

struct OwnedCaseV1 {
    name: String,
    state: Vec<f32>,
    object_features: Vec<f32>,
    object_card_ids: Vec<i64>,
    object_groups: Vec<i64>,
    object_node_ids: Vec<i64>,
    edge_features: Vec<f32>,
    edge_source_indices: Vec<i64>,
    edge_target_indices: Vec<i64>,
    action_features: Vec<f32>,
    action_ref_features: Vec<f32>,
    action_ref_card_ids: Vec<i64>,
    action_ref_action_indices: Vec<i64>,
    action_ref_node_indices: Vec<i64>,
}

impl OwnedCaseV1 {
    fn from_fixture_v1(case: FixtureCaseV1) -> Self {
        let tensors = case.tensors;
        Self {
            name: case.name,
            state: decode_f32_hex_v1(&tensors.state.f32_le_hex),
            object_features: decode_f32_hex_v1(&tensors.object_features.f32_le_hex),
            object_card_ids: tensors.object_card_ids.i64_values,
            object_groups: tensors.object_groups.i64_values,
            object_node_ids: tensors.object_node_ids.i64_values,
            edge_features: decode_f32_hex_v1(&tensors.edge_features.f32_le_hex),
            edge_source_indices: tensors.edge_source_indices.i64_values,
            edge_target_indices: tensors.edge_target_indices.i64_values,
            action_features: decode_f32_hex_v1(&tensors.action_features.f32_le_hex),
            action_ref_features: decode_f32_hex_v1(&tensors.action_ref_features.f32_le_hex),
            action_ref_card_ids: tensors.action_ref_card_ids.i64_values,
            action_ref_action_indices: tensors.action_ref_action_indices.i64_values,
            action_ref_node_indices: tensors.action_ref_node_indices.i64_values,
        }
    }

    fn view(&self) -> net::NativeEncodedDecisionViewV1<'_> {
        net::NativeEncodedDecisionViewV1::from_slices_unvalidated(
            net::NativeEncodedDecisionSchemaV1::contract_v1(),
            &self.state,
            &self.object_features,
            &self.object_card_ids,
            &self.object_groups,
            &self.object_node_ids,
            &self.edge_features,
            &self.edge_source_indices,
            &self.edge_target_indices,
            &self.action_features,
            &self.action_ref_features,
            &self.action_ref_card_ids,
            &self.action_ref_action_indices,
            &self.action_ref_node_indices,
        )
    }

    fn action_count(&self) -> usize {
        self.action_features.len() / net::ACTION_FEATURE_DIM_V1
    }

    fn shape_json(&self) -> Value {
        json!({
            "name": self.name,
            "objects": self.object_card_ids.len(),
            "edges": self.edge_source_indices.len(),
            "actions": self.action_count(),
            "action_refs": self.action_ref_action_indices.len(),
        })
    }
}

fn decode_f32_hex_v1(raw: &str) -> Vec<f32> {
    assert!(raw.len().is_multiple_of(8), "unaligned f32 hex");
    let mut values = Vec::with_capacity(raw.len() / 8);
    for chunk in raw.as_bytes().chunks_exact(8) {
        let mut bytes = [0_u8; 4];
        for (byte, pair) in bytes.iter_mut().zip(chunk.chunks_exact(2)) {
            let text = std::str::from_utf8(pair).expect("ascii hex");
            *byte = u8::from_str_radix(text, 16).expect("hex pair");
        }
        values.push(f32::from_le_bytes(bytes));
    }
    values
}

/// The fixture's real replay cases (every `synthetic-` case is skipped, as in
/// the Burn packed-forward tests that load the same file).
fn load_real_cases_v1() -> Vec<OwnedCaseV1> {
    let document: FixtureDocumentV1 = serde_json::from_slice(FIXTURE_BYTES).expect("parses");
    let mut cases = Vec::new();
    for case in document.cases {
        if !case.name.starts_with("synthetic-") {
            cases.push(OwnedCaseV1::from_fixture_v1(case));
        }
    }
    assert!(!cases.is_empty(), "no real replay cases");
    cases
}

fn fixture_json_v1(cases: &[OwnedCaseV1]) -> Value {
    let actions: usize = cases.iter().map(OwnedCaseV1::action_count).sum();
    let objects: usize = cases.iter().map(|case| case.object_card_ids.len()).sum();
    json!({
        "path": FIXTURE_PATH,
        "sha256": format!("{:x}", Sha256::digest(FIXTURE_BYTES)),
        "real_cases": cases.len(),
        "decisions_per_pass": cases.len(),
        "objects_per_pass": objects,
        "actions_per_pass": actions,
    })
}

/// The w64 production train state: the frozen snapshot loaded through the
/// production loader. Returns the state and its snapshot digest.
fn load_narrow_state_v1() -> (train::NativePolicyValueTrainStateV1, String) {
    let config = net::NativePolicyValueModelConfigV1::contract_v1();
    let model = net::NativePolicyValueNetV1::runner_fixed_v1(config).expect("w64 model");
    let mut state = train::NativePolicyValueTrainStateV1::new_v1(model).expect("w64 state");
    let (manifest, parameters) = snapshot::common_model_snapshot_paths_v1();
    let record = snapshot::load_common_model_snapshot_v1(&manifest, &parameters, &mut state)
        .expect("w64 snapshot loads");
    (state, record.snapshot_sha256().to_owned())
}

/// The w128 model from `data/wide_model_snapshot_w128` through the wide
/// loader. Returns the model and its snapshot digest.
fn load_wide_model_v1() -> (net::NativePolicyValueNetWideV1, String) {
    let (manifest, parameters) = snapshot::wide_model_snapshot_paths_v1();
    let (model, record) = snapshot::build_wide_model_candidate_v1(&manifest, &parameters)
        .expect("w128 snapshot loads");
    (model, record.snapshot_sha256().to_owned())
}

enum ModelV1<'a> {
    W64(&'a net::NativePolicyValueNetV1),
    W128(&'a net::NativePolicyValueNetWideV1),
}

impl ModelV1<'_> {
    fn forward(
        &self,
        view: net::NativeEncodedDecisionViewV1<'_>,
    ) -> net::NativePolicyValueOutputV1 {
        match self {
            Self::W64(model) => model.forward_v1(view).expect("w64 forward"),
            Self::W128(model) => model.forward_wide_v1(view).expect("w128 forward"),
        }
    }
}

/// Calling thread's CPU time (user plus kernel) in nanoseconds.
#[cfg(windows)]
fn thread_cpu_ns_v1() -> Option<u64> {
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn GetThreadTimes(
            thread: *mut std::ffi::c_void,
            created: *mut FileTime,
            exited: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }
    let (mut created, mut exited, mut kernel, mut user) = (
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
    );
    // SAFETY: the current-thread pseudo handle is always valid and every
    // FILETIME out-pointer is a live, writable local.
    let ok = unsafe {
        GetThreadTimes(
            GetCurrentThread(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    };
    if ok == 0 {
        return None;
    }
    let ticks = |time: FileTime| (u64::from(time.high) << 32) | u64::from(time.low);
    Some((ticks(kernel) + ticks(user)) * 100)
}

/// Calling thread's CPU time (user plus system) in nanoseconds.
#[cfg(target_os = "linux")]
fn thread_cpu_ns_v1() -> Option<u64> {
    #[repr(C)]
    struct Timespec {
        sec: std::ffi::c_long,
        nano: std::ffi::c_long,
    }
    unsafe extern "C" {
        fn clock_gettime(clock: i32, out: *mut Timespec) -> i32;
    }
    let mut out = Timespec { sec: 0, nano: 0 };
    // SAFETY: CLOCK_THREAD_CPUTIME_ID is 3 on Linux; out is writable.
    if unsafe { clock_gettime(3, &mut out) } != 0 {
        return None;
    }
    let seconds = u64::try_from(out.sec).ok()?;
    let nanos = u64::try_from(out.nano).ok()?;
    Some(seconds * 1_000_000_000 + nanos)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn thread_cpu_ns_v1() -> Option<u64> {
    None
}

/// Wall and thread-CPU totals for one arm. CPU stays None when the platform
/// has no thread clock.
struct TotalsV1 {
    wall_ns: f64,
    cpu_ns: Option<f64>,
    units: usize,
}

impl TotalsV1 {
    fn new() -> Self {
        Self {
            wall_ns: 0.0,
            cpu_ns: Some(0.0),
            units: 0,
        }
    }

    fn add(&mut self, wall_ns: f64, cpu: Option<(u64, u64)>, units: usize) {
        self.wall_ns += wall_ns;
        self.units += units;
        let delta = cpu.map(|(start, end)| end.saturating_sub(start) as f64);
        self.cpu_ns = self.cpu_ns.zip(delta).map(|(total, delta)| total + delta);
    }

    fn json(&self) -> Value {
        let units = self.units.max(1) as f64;
        json!({
            "units": self.units,
            "wall_ns_per_unit": self.wall_ns / units,
            "thread_cpu_ns_per_unit": self.cpu_ns.map(|cpu| cpu / units),
            "thread_cpu_over_wall": self.cpu_ns.map(|cpu| cpu / self.wall_ns.max(1.0)),
        })
    }
}

/// Starts a wall and thread-CPU interval.
fn start_clock_v1() -> (Instant, Option<u64>) {
    let cpu = thread_cpu_ns_v1();
    (Instant::now(), cpu)
}

/// Ends an interval: elapsed wall ns and the (start, end) CPU readings.
fn stop_clock_v1(clock: (Instant, Option<u64>)) -> MeasuredV1 {
    let wall = clock.0.elapsed().as_nanos() as f64;
    let cpu = clock.1.zip(thread_cpu_ns_v1());
    (wall, cpu)
}

fn sorted_v1(samples: &[f64]) -> Vec<f64> {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted
}

/// Nearest-rank quantile of an ascending, nonempty sample.
fn quantile_v1(sorted: &[f64], q: f64) -> f64 {
    let last = sorted.len() - 1;
    let rank = (q * last as f64).round() as usize;
    sorted[rank.min(last)]
}

fn median_v1(samples: &[f64]) -> f64 {
    quantile_v1(&sorted_v1(samples), 0.5)
}

fn summary_v1(samples: &[f64]) -> Value {
    let sorted = sorted_v1(samples);
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    json!({
        "count": sorted.len(),
        "median": quantile_v1(&sorted, 0.5),
        "p10": quantile_v1(&sorted, 0.1),
        "p90": quantile_v1(&sorted, 0.9),
        "mean": mean,
        "min": sorted[0],
        "max": sorted[sorted.len() - 1],
    })
}

fn ratio_json_v1(numerator: &[f64], denominator: &[f64]) -> Value {
    let (top, bottom) = (sorted_v1(numerator), sorted_v1(denominator));
    json!({
        "median": quantile_v1(&top, 0.5) / quantile_v1(&bottom, 0.5),
        "p10": quantile_v1(&top, 0.1) / quantile_v1(&bottom, 0.1),
        "p90": quantile_v1(&top, 0.9) / quantile_v1(&bottom, 0.9),
    })
}

fn cpu_ratio_v1(numerator: &TotalsV1, denominator: &TotalsV1) -> Option<f64> {
    let top = numerator.cpu_ns? / numerator.units.max(1) as f64;
    let bottom = denominator.cpu_ns? / denominator.units.max(1) as f64;
    Some(top / bottom)
}

fn env_usize_v1(name: &str) -> Option<usize> {
    let raw = std::env::var(name).ok()?;
    Some(raw.trim().parse().expect("count env var"))
}

fn env_f64_v1(name: &str) -> Option<f64> {
    let raw = std::env::var(name).ok()?;
    Some(raw.trim().parse().expect("seconds env var"))
}

/// Timed repetitions per arm: `HARNESS_REPS` when set, otherwise enough to
/// spend about `HARNESS_TARGET_SECONDS` (or `default_seconds`) timing every
/// arm once per repetition, clamped to `[min, max]`.
fn reps_v1(ns_per_rep: f64, default_seconds: f64, min: usize, max: usize) -> usize {
    if let Some(reps) = env_usize_v1("HARNESS_REPS") {
        return reps.max(1);
    }
    let seconds = env_f64_v1("HARNESS_TARGET_SECONDS").unwrap_or(default_seconds);
    let reps = (seconds * 1.0e9 / ns_per_rep.max(1.0)).ceil() as usize;
    reps.clamp(min, max)
}

fn host_json_v1() -> Value {
    #[cfg(target_arch = "x86_64")]
    let avx2 = std::arch::is_x86_feature_detected!("avx2");
    #[cfg(not(target_arch = "x86_64"))]
    let avx2 = false;
    let parallelism = std::thread::available_parallelism().map(usize::from).ok();
    json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "avx2": avx2,
        "available_parallelism": parallelism,
        "debug_assertions": cfg!(debug_assertions),
        "thread_cpu_clock": thread_cpu_ns_v1().is_some(),
    })
}

/// One pass over every case; pushes each case's forward ns and returns the
/// pass's summed ns.
fn forward_pass_v1(model: &ModelV1<'_>, cases: &[OwnedCaseV1], per_case: &mut [Vec<f64>]) -> f64 {
    let mut pass_ns = 0.0;
    for (case, samples) in cases.iter().zip(per_case.iter_mut()) {
        let start = Instant::now();
        drop(black_box(model.forward(black_box(case.view()))));
        let elapsed = start.elapsed().as_nanos() as f64;
        samples.push(elapsed);
        pass_ns += elapsed;
    }
    pass_ns
}

struct ForwardArmV1 {
    per_pass_ns_per_decision: Vec<f64>,
    per_case_ns: Vec<Vec<f64>>,
    totals: TotalsV1,
}

impl ForwardArmV1 {
    fn new(cases: usize) -> Self {
        Self {
            per_pass_ns_per_decision: Vec::new(),
            per_case_ns: vec![Vec::new(); cases],
            totals: TotalsV1::new(),
        }
    }

    fn run_block(&mut self, model: &ModelV1<'_>, cases: &[OwnedCaseV1], passes: usize) {
        let decisions = cases.len() as f64;
        let clock = start_clock_v1();
        for _ in 0..passes {
            let pass_ns = forward_pass_v1(model, cases, &mut self.per_case_ns);
            self.per_pass_ns_per_decision.push(pass_ns / decisions);
        }
        let (wall, cpu) = stop_clock_v1(clock);
        self.totals.add(wall, cpu, passes * cases.len());
    }
}

/// M1: w64 vs w128 CPU forward cost per decision on the real fixture cases.
#[test]
#[ignore = "timing harness"]
fn m1_forward_w64_vs_w128_cpu_cost() {
    let started = Instant::now();
    let cases = load_real_cases_v1();
    let (narrow_state, narrow_snapshot) = load_narrow_state_v1();
    let (wide, wide_snapshot) = load_wide_model_v1();
    let models = [ModelV1::W64(narrow_state.model_v1()), ModelV1::W128(&wide)];
    for model in &models {
        for case in &cases {
            let output = model.forward(case.view());
            assert_eq!(output.logits.len(), case.action_count());
            assert!(output.value.is_finite());
            assert!(output.logits.iter().all(|logit| logit.is_finite()));
        }
    }

    let warmup = env_usize_v1("HARNESS_WARMUP").unwrap_or(30).max(1);
    let mut warm_ns_per_pass = 0.0;
    for model in &models {
        let mut scratch = ForwardArmV1::new(cases.len());
        scratch.run_block(model, &cases, warmup);
        warm_ns_per_pass += scratch.totals.wall_ns / warmup as f64;
    }
    let block = env_usize_v1("HARNESS_BLOCK").unwrap_or(25).max(1);
    let reps = reps_v1(warm_ns_per_pass, 40.0, block * 4, 10_000_000);
    let blocks = reps.div_ceil(block);

    let mut arms = [
        ForwardArmV1::new(cases.len()),
        ForwardArmV1::new(cases.len()),
    ];
    for index in 0..blocks {
        let order = if index.is_multiple_of(2) {
            [0, 1]
        } else {
            [1, 0]
        };
        for arm in order {
            arms[arm].run_block(&models[arm], &cases, block);
        }
    }

    let [narrow, wide_arm] = &arms;
    let mut per_case = Vec::with_capacity(cases.len());
    for (index, case) in cases.iter().enumerate() {
        let w64 = median_v1(&narrow.per_case_ns[index]);
        let w128 = median_v1(&wide_arm.per_case_ns[index]);
        let mut row = case.shape_json();
        row["w64_median_ns"] = json!(w64);
        row["w128_median_ns"] = json!(w128);
        row["ratio_w128_over_w64"] = json!(w128 / w64);
        per_case.push(row);
    }
    let result = json!({
        "harness": "w128_cost_harness_v1",
        "measurement": "M1 CPU forward per decision, w64 vs w128",
        "forward_paths": {
            "w64": "NativePolicyValueNetV1::forward_v1",
            "w128": "NativePolicyValueNetWideV1::forward_wide_v1",
        },
        "fixture": fixture_json_v1(&cases),
        "host": host_json_v1(),
        "config": {
            "passes_per_arm": blocks * block,
            "block_passes": block,
            "blocks_per_arm": blocks,
            "warmup_passes_per_arm": warmup,
            "order": "ABBA blocks (A = w64, B = w128), single thread",
            "sample": "one pass over every real case, ns per decision",
        },
        "w64": {
            "architecture": net::MODEL_ARCHITECTURE_VERSION_V1,
            "hidden_dim": net::HIDDEN_DIM_V1,
            "card_embedding_dim": net::CARD_EMBEDDING_DIM_V1,
            "parameters": net::PARAMETER_COUNT_V1,
            "snapshot_sha256": narrow_snapshot,
            "ns_per_decision": summary_v1(&narrow.per_pass_ns_per_decision),
            "totals_per_decision": narrow.totals.json(),
        },
        "w128": {
            "architecture": net::W_MODEL_ARCHITECTURE_VERSION_V1,
            "hidden_dim": net::W_HIDDEN_DIM_V1,
            "card_embedding_dim": net::W_CARD_EMBEDDING_DIM_V1,
            "parameters": net::W_PARAMETER_COUNT_V1,
            "snapshot_sha256": wide_snapshot,
            "ns_per_decision": summary_v1(&wide_arm.per_pass_ns_per_decision),
            "totals_per_decision": wide_arm.totals.json(),
        },
        "ratio_w128_over_w64": ratio_json_v1(
            &wide_arm.per_pass_ns_per_decision,
            &narrow.per_pass_ns_per_decision,
        ),
        "thread_cpu_ratio_w128_over_w64": cpu_ratio_v1(&wide_arm.totals, &narrow.totals),
        "per_case": per_case,
        "elapsed_seconds": started.elapsed().as_secs_f64(),
    });
    println!("{result}");
}

/// Per-arm M2 samples: live-state digest, owned snapshot and payload encode.
struct StateArmV1 {
    digest_ns: Vec<f64>,
    snapshot_ns: Vec<f64>,
    encode_ns: Vec<f64>,
    digest_totals: TotalsV1,
    snapshot_totals: TotalsV1,
    encode_totals: TotalsV1,
    payload_bytes: usize,
    snapshot_parameter_bytes: usize,
}

impl StateArmV1 {
    fn new() -> Self {
        Self {
            digest_ns: Vec::new(),
            snapshot_ns: Vec::new(),
            encode_ns: Vec::new(),
            digest_totals: TotalsV1::new(),
            snapshot_totals: TotalsV1::new(),
            encode_totals: TotalsV1::new(),
            payload_bytes: 0,
            snapshot_parameter_bytes: 0,
        }
    }

    fn json(&self) -> Value {
        json!({
            "state_sha256_ns": summary_v1(&self.digest_ns),
            "state_sha256_totals": self.digest_totals.json(),
            "snapshot_ns": summary_v1(&self.snapshot_ns),
            "snapshot_totals": self.snapshot_totals.json(),
            "payload_encode_ns": summary_v1(&self.encode_ns),
            "payload_encode_totals": self.encode_totals.json(),
            "payload_bytes": self.payload_bytes,
            "snapshot_parameter_bytes": self.snapshot_parameter_bytes,
        })
    }
}

enum TrainStateV1<'a> {
    W64(&'a train::NativePolicyValueTrainStateV1),
    W128(&'a train::NativePolicyValueTrainStateWideV1),
}

impl TrainStateV1<'_> {
    fn digest(&self) -> [u8; 32] {
        match self {
            Self::W64(state) => state.state_sha256_v1().expect("w64 digest"),
            Self::W128(state) => state.state_sha256_v1().expect("w128 digest"),
        }
    }

    fn snapshot(&self) -> train::NativePolicyValueTrainSnapshotV1 {
        match self {
            Self::W64(state) => state.snapshot_v1().expect("w64 snapshot"),
            Self::W128(state) => state.snapshot_v1().expect("w128 snapshot"),
        }
    }

    fn encode(&self, owned: &train::NativePolicyValueTrainSnapshotV1) -> Vec<u8> {
        let encoded = match self {
            Self::W64(_) => payload::encode_native_train_state_payload_v1(owned),
            Self::W128(_) => payload::encode_native_train_state_payload_wide_v1(owned),
        };
        encoded.expect("payload encodes").bytes
    }

    /// One digest, one snapshot and one encode, each timed separately.
    fn measure(&self, arm: &mut StateArmV1) {
        let clock = start_clock_v1();
        black_box(self.digest());
        let (wall, cpu) = stop_clock_v1(clock);
        arm.digest_ns.push(wall);
        arm.digest_totals.add(wall, cpu, 1);

        let clock = start_clock_v1();
        let owned = black_box(self.snapshot());
        let (wall, cpu) = stop_clock_v1(clock);
        arm.snapshot_ns.push(wall);
        arm.snapshot_totals.add(wall, cpu, 1);

        let clock = start_clock_v1();
        let bytes = black_box(self.encode(&owned));
        let (wall, cpu) = stop_clock_v1(clock);
        arm.encode_ns.push(wall);
        arm.encode_totals.add(wall, cpu, 1);

        arm.payload_bytes = bytes.len();
        arm.snapshot_parameter_bytes = owned
            .parameters
            .iter()
            .map(|tensor| tensor.values.len() * 4)
            .sum();
    }
}

/// M2: train-state digest, owned snapshot and payload encode, w64 vs w128.
#[test]
#[ignore = "timing harness"]
fn m2_train_state_digest_and_payload_cost() {
    let started = Instant::now();
    let (narrow_state, narrow_snapshot) = load_narrow_state_v1();
    let (wide_model, wide_snapshot) = load_wide_model_v1();
    let wide_state =
        train::NativePolicyValueTrainStateWideV1::new_wide_v1(wide_model).expect("w128 state");
    let states = [
        TrainStateV1::W64(&narrow_state),
        TrainStateV1::W128(&wide_state),
    ];

    let warmup = env_usize_v1("HARNESS_WARMUP").unwrap_or(2).max(1);
    let warm_clock = Instant::now();
    for state in &states {
        let mut scratch = StateArmV1::new();
        for _ in 0..warmup {
            state.measure(&mut scratch);
        }
    }
    let warm_ns_per_rep = warm_clock.elapsed().as_nanos() as f64 / warmup as f64;
    let reps = reps_v1(warm_ns_per_rep, 30.0, 10, 100_000);

    let mut arms = [StateArmV1::new(), StateArmV1::new()];
    for rep in 0..reps {
        let order = if rep.is_multiple_of(2) {
            [0, 1]
        } else {
            [1, 0]
        };
        for arm in order {
            states[arm].measure(&mut arms[arm]);
        }
    }

    let [narrow, wide] = &arms;
    let result = json!({
        "harness": "w128_cost_harness_v1",
        "measurement": "M2 train-state digest, snapshot and payload encode, w64 vs w128",
        "paths": {
            "state_sha256": "NativePolicyValueTrainState{,Wide}V1::state_sha256_v1",
            "snapshot": "NativePolicyValueTrainState{,Wide}V1::snapshot_v1",
            "payload_encode": "encode_native_train_state_payload{,_wide}_v1",
        },
        "host": host_json_v1(),
        "config": {
            "reps_per_arm": reps,
            "warmup_reps_per_arm": warmup,
            "order": "ABBA per repetition (A = w64, B = w128), single thread",
            "state": "snapshot-loaded parameters, Adam step 0, zero moments",
        },
        "w64": {
            "architecture": net::MODEL_ARCHITECTURE_VERSION_V1,
            "parameters": net::PARAMETER_COUNT_V1,
            "snapshot_sha256": narrow_snapshot,
            "costs": narrow.json(),
        },
        "w128": {
            "architecture": net::W_MODEL_ARCHITECTURE_VERSION_V1,
            "parameters": net::W_PARAMETER_COUNT_V1,
            "snapshot_sha256": wide_snapshot,
            "costs": wide.json(),
        },
        "ratio_w128_over_w64": {
            "state_sha256": ratio_json_v1(&wide.digest_ns, &narrow.digest_ns),
            "snapshot": ratio_json_v1(&wide.snapshot_ns, &narrow.snapshot_ns),
            "payload_encode": ratio_json_v1(&wide.encode_ns, &narrow.encode_ns),
            "payload_bytes": wide.payload_bytes as f64 / narrow.payload_bytes as f64,
        },
        "elapsed_seconds": started.elapsed().as_secs_f64(),
    });
    println!("{result}");
}

type MeasuredV1 = (f64, Option<(u64, u64)>);

/// One timed forward pass over every case.
fn timed_forward_v1(
    model: &ModelV1<'_>,
    cases: &[OwnedCaseV1],
    per_case: &mut [Vec<f64>],
) -> MeasuredV1 {
    let clock = start_clock_v1();
    black_box(forward_pass_v1(model, cases, per_case));
    stop_clock_v1(clock)
}

/// One timed train step on a fresh clone of `initial` (clone untimed).
fn timed_step_v1(
    initial: &train::NativePolicyValueTrainStateV1,
    batch: &[train::NativePolicyPhysicalDecisionV1<'_>],
) -> (MeasuredV1, f32) {
    let mut state = initial.clone();
    let clock = start_clock_v1();
    let result = state.train_step_v1(batch, VALUE_COEFFICIENT, LEARNING_RATE);
    let measured = stop_clock_v1(clock);
    let result = black_box(result.expect("narrow train step"));
    assert_eq!(result.adam_step, 1);
    assert!(result.loss.is_finite());
    (measured, result.loss)
}

/// M3: one narrow CPU train step (forward with tape, backward, Adam) on the
/// real fixture batch, against a narrow forward of the same batch and a
/// one-decision train step. Every step starts from a clone of the same
/// snapshot-loaded state; the clone is outside the timed interval.
#[test]
#[ignore = "timing harness"]
fn m3_narrow_cpu_train_step_vs_forward_cost() {
    let started = Instant::now();
    let cases = load_real_cases_v1();
    let (initial, narrow_snapshot) = load_narrow_state_v1();
    let model = ModelV1::W64(initial.model_v1());

    let mut expected = Vec::with_capacity(cases.len());
    for case in &cases {
        let output = model.forward(case.view());
        let logit_bits: Vec<u32> = output.logits.iter().map(|logit| logit.to_bits()).collect();
        expected.push((logit_bits, output.value.to_bits()));
    }
    let mut substeps = Vec::with_capacity(cases.len());
    for (index, (case, (logit_bits, value_bits))) in cases.iter().zip(&expected).enumerate() {
        substeps.push(train::NativePolicySubstepV1 {
            forward: train::NativePolicyForwardInputV1::Encoded(Box::new(case.view())),
            selected_action_index: index % case.action_count(),
            expected_raw_action_logit_bits: logit_bits,
            expected_value_bits: *value_bits,
        });
    }
    let mut groups = Vec::with_capacity(cases.len());
    for (index, substep) in substeps.chunks(1).enumerate() {
        groups.push(train::NativePolicyPhysicalDecisionV1 {
            substeps: substep,
            terminal_return: if index.is_multiple_of(2) { 1 } else { -1 },
            baseline_bits: 0,
        });
    }

    let warmup = env_usize_v1("HARNESS_WARMUP").unwrap_or(3).max(1);
    let mut scratch: Vec<Vec<f64>> = vec![Vec::new(); cases.len()];
    let warm_clock = Instant::now();
    for _ in 0..warmup {
        timed_forward_v1(&model, &cases, &mut scratch);
        timed_step_v1(&initial, &groups);
        timed_step_v1(&initial, &groups[..1]);
    }
    let warm_ns_per_rep = warm_clock.elapsed().as_nanos() as f64 / warmup as f64;
    let reps = reps_v1(warm_ns_per_rep, 30.0, 20, 1_000_000);

    let mut per_case: Vec<Vec<f64>> = vec![Vec::new(); cases.len()];
    let mut forward_ns = Vec::with_capacity(reps);
    let mut batch_ns = Vec::with_capacity(reps);
    let mut single_ns = Vec::with_capacity(reps);
    let mut forward_totals = TotalsV1::new();
    let mut batch_totals = TotalsV1::new();
    let mut single_totals = TotalsV1::new();
    let mut batch_loss = f32::NAN;
    for rep in 0..reps {
        let order = if rep.is_multiple_of(2) {
            [0, 1, 2]
        } else {
            [2, 1, 0]
        };
        for arm in order {
            match arm {
                0 => {
                    let (wall, cpu) = timed_forward_v1(&model, &cases, &mut per_case);
                    forward_ns.push(wall);
                    forward_totals.add(wall, cpu, cases.len());
                }
                1 => {
                    let ((wall, cpu), loss) = timed_step_v1(&initial, &groups);
                    batch_ns.push(wall);
                    batch_totals.add(wall, cpu, 1);
                    batch_loss = loss;
                }
                _ => {
                    let ((wall, cpu), _) = timed_step_v1(&initial, &groups[..1]);
                    single_ns.push(wall);
                    single_totals.add(wall, cpu, 1);
                }
            }
        }
    }

    let decisions = cases.len() as f64;
    let forward_median = median_v1(&forward_ns);
    let batch_median = median_v1(&batch_ns);
    let single_median = median_v1(&single_ns);
    let marginal = (batch_median - single_median) / (decisions - 1.0).max(1.0);
    let result = json!({
        "harness": "w128_cost_harness_v1",
        "measurement": "M3 narrow CPU train step vs forward on the same real batch",
        "paths": {
            "forward": "NativePolicyValueNetV1::forward_v1",
            "train_step": "NativePolicyValueTrainStateV1::train_step_v1 (1 worker, sequential backward)",
        },
        "fixture": fixture_json_v1(&cases),
        "host": host_json_v1(),
        "config": {
            "reps": reps,
            "warmup_reps": warmup,
            "order": "ABC CBA per repetition (forward, batch step, one-decision step)",
            "batch": "one physical decision per real case, one substep each",
            "value_coefficient": VALUE_COEFFICIENT,
            "learning_rate": LEARNING_RATE,
            "architecture": net::MODEL_ARCHITECTURE_VERSION_V1,
            "snapshot_sha256": narrow_snapshot,
        },
        "forward_batch_ns": summary_v1(&forward_ns),
        "forward_totals_per_decision": forward_totals.json(),
        "train_step_batch_ns": summary_v1(&batch_ns),
        "train_step_batch_totals": batch_totals.json(),
        "train_step_one_decision_ns": summary_v1(&single_ns),
        "train_step_one_decision_totals": single_totals.json(),
        "batch_loss": batch_loss,
        "derived": {
            "forward_median_ns_per_decision": forward_median / decisions,
            "train_step_over_forward_batch_median": batch_median / forward_median,
            "train_step_marginal_ns_per_decision": marginal,
            "train_step_fixed_ns_estimate": single_median - marginal,
            "marginal_over_forward_per_decision": marginal / (forward_median / decisions),
        },
        "elapsed_seconds": started.elapsed().as_secs_f64(),
    });
    println!("{result}");
}
