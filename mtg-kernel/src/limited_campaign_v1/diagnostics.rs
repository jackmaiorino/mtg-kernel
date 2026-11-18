//! `--stage-diagnostics` (design Section 5.2 item 4): stage timers and a
//! JSONL sidecar. Nothing here enters a receipt, checkpoint or evaluation
//! file, so the byte-compared outputs are the same with diagnostics on or
//! off.
use serde::Serialize;
use serde_json::{json, Value};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

pub const DIAGNOSTICS_FILE_V1: &str = "stage-diagnostics.jsonl";
pub const DIAGNOSTICS_SCHEMA_V1: &str = "limited-campaign-stage-diagnostics/v1";

/// The JSONL writer. Each line is written at once, so a killed run keeps the
/// completed lines.
pub struct DiagnosticsV1 {
    started: Instant,
    file: Mutex<Option<File>>,
}

impl DiagnosticsV1 {
    /// Opens (appending) `stage-diagnostics.jsonl` under `root`.
    pub fn open(root: &Path) -> Result<Self, String> {
        let file = File::options()
            .append(true)
            .create(true)
            .open(root.join(DIAGNOSTICS_FILE_V1))
            .map_err(|error| format!("stage diagnostics: {error}"))?;
        Ok(Self {
            started: Instant::now(),
            file: Mutex::new(Some(file)),
        })
    }

    pub fn record(&self, event: Value) {
        let mut file = self.file.lock().expect("stage diagnostics writer panicked");
        if let Some(writer) = file.as_mut() {
            let mut row = json!({
                "schema": DIAGNOSTICS_SCHEMA_V1,
                "elapsed_ns": elapsed_ns(self.started),
                "event": event,
            })
            .to_string()
            .into_bytes();
            row.push(b'\n');
            if let Err(error) = writer.write_all(&row) {
                *file = None;
                eprintln!("stage diagnostics write failed: {error}");
            }
        }
    }
}

pub fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// Times `operation` as one phase when diagnostics are on.
pub fn measure_phase<T>(
    diagnostics: Option<&DiagnosticsV1>,
    phase: &str,
    counter: Option<&mut StageCounterV1>,
    operation: impl FnOnce() -> T,
) -> T {
    if diagnostics.is_none() && counter.is_none() {
        return operation();
    }
    if let Some(diagnostics) = diagnostics {
        diagnostics.record(json!({"phase": phase, "checkpoint": "started"}));
    }
    let started = Instant::now();
    let result = operation();
    let duration_ns = elapsed_ns(started);
    if let Some(diagnostics) = diagnostics {
        diagnostics
            .record(json!({"phase": phase, "checkpoint": "returned", "duration_ns": duration_ns}));
    }
    if let Some(counter) = counter {
        counter.add(duration_ns);
    }
    result
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct StageCounterV1 {
    pub count: u64,
    pub total_ns: u64,
    pub max_ns: u64,
}

impl StageCounterV1 {
    pub fn add(&mut self, duration_ns: u64) {
        self.count += 1;
        self.total_ns = self.total_ns.saturating_add(duration_ns);
        self.max_ns = self.max_ns.max(duration_ns);
    }
    pub fn merge(&mut self, other: &Self) {
        self.count += other.count;
        self.total_ns = self.total_ns.saturating_add(other.total_ns);
        self.max_ns = self.max_ns.max(other.max_ns);
    }
}

/// Times `operation` into `counter` when diagnostics are on.
pub fn measure_stage<T>(counter: Option<&mut StageCounterV1>, operation: impl FnOnce() -> T) -> T {
    let Some(counter) = counter else {
        return operation();
    };
    let started = Instant::now();
    let result = operation();
    counter.add(elapsed_ns(started));
    result
}

/// Per-game stages.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct GameStagesV1 {
    pub reset: StageCounterV1,
    pub current_response: StageCounterV1,
    pub encode: StageCounterV1,
    pub authenticated_input: StageCounterV1,
    pub model_forward: StageCounterV1,
    pub sample: StageCounterV1,
    pub apply: StageCounterV1,
}

impl GameStagesV1 {
    pub fn merge(&mut self, other: &Self) {
        self.reset.merge(&other.reset);
        self.current_response.merge(&other.current_response);
        self.encode.merge(&other.encode);
        self.authenticated_input.merge(&other.authenticated_input);
        self.model_forward.merge(&other.model_forward);
        self.sample.merge(&other.sample);
        self.apply.merge(&other.apply);
    }
}

/// Process-wide stages outside the games.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct RunStagesV1 {
    pub games: GameStagesV1,
    pub collection: StageCounterV1,
    pub update: StageCounterV1,
    pub io: StageCounterV1,
    pub dev_panel: StageCounterV1,
}
