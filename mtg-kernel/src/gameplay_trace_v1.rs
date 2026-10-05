//! Opt-in diagnostics. A sampled record is completed by the real engine step.
//! Never serialize an engine state, opposing actor view, or environment RNG.
use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
use crate::rl::PlayerSeatV1;
use crate::rl_session::FastActorDecisionV1;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const SCHEMA_V1: &str = "mtg-kernel-gameplay-decision-trace/v1";
const FOOTER_RESERVE: usize = 1024;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GameplayTraceConfigV1 {
    pub path: PathBuf,
    pub run_id: String,
    pub max_decisions: u64,
    pub max_bytes: usize,
    pub max_record_bytes: usize,
}

/// Deliberately absent from checkpoint and primary output DTOs.
pub struct GameplayTraceV1 {
    file: File,
    config: GameplayTraceConfigV1,
    bytes: usize,
    written: u64,
    omitted: u64,
    stopped: Option<&'static str>,
    previous_actor_record: [Option<u64>; 2],
}
pub(crate) type TraceHandle = Arc<Mutex<GameplayTraceV1>>;

impl GameplayTraceV1 {
    pub(crate) fn open(
        config: GameplayTraceConfigV1,
        identity: Value,
    ) -> Result<TraceHandle, String> {
        if !config.path.is_absolute()
            || config.run_id.is_empty()
            || config.run_id.len() > 128
            || config.max_decisions == 0
            || config.max_decisions > 100_000
            || !(4096..=256 * 1024 * 1024).contains(&config.max_bytes)
            || !(1024..=8 * 1024 * 1024).contains(&config.max_record_bytes)
            || config.max_record_bytes > config.max_bytes - FOOTER_RESERVE
        {
            return Err("invalid bounded gameplay trace configuration".into());
        }
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&config.path)
            .map_err(|e| format!("create diagnostic sidecar: {e}"))?;
        let mut trace = Self {
            file,
            config,
            bytes: 0,
            written: 0,
            omitted: 0,
            stopped: None,
            previous_actor_record: [None; 2],
        };
        let header = json!({"schema": SCHEMA_V1, "kind":"header", "run_id":trace.config.run_id,
            "limits":trace.config,"identity":identity,
            "visibility":"each decision and transition belong only to the recorded actor; private diagnostic file"});
        trace
            .write_value(&header)
            .map_err(|_| "trace header exceeds limit or cannot be written")?;
        Ok(Arc::new(Mutex::new(trace)))
    }

    pub(crate) fn accepting(handle: &TraceHandle) -> bool {
        let Ok(mut trace) = handle.lock() else {
            return false;
        };
        if trace.stopped.is_some() || trace.written >= trace.config.max_decisions {
            if trace.stopped.is_none() {
                trace.stopped = Some("decision_limit");
            }
            trace.omitted += 1;
            false
        } else {
            true
        }
    }

    fn write_value(&mut self, value: &Value) -> Result<(), ()> {
        let mut buffer = LimitedBuffer {
            bytes: Vec::new(),
            cap: self.config.max_record_bytes,
        };
        serde_json::to_writer(&mut buffer, value).map_err(|_| ())?;
        buffer.write_all(b"\n").map_err(|_| ())?;
        if self.bytes + buffer.bytes.len() > self.config.max_bytes - FOOTER_RESERVE {
            return Err(());
        }
        self.file.write_all(&buffer.bytes).map_err(|_| ())?;
        self.bytes += buffer.bytes.len();
        Ok(())
    }

    pub(crate) fn record(handle: &TraceHandle, mut value: Value) {
        let Ok(mut trace) = handle.lock() else {
            return;
        };
        if trace.stopped.is_some() {
            trace.omitted += 1;
            return;
        }
        let actor = if value["actor"] == "p0" { 0 } else { 1 };
        let index = trace.written;
        value["trace_index"] = json!(index);
        value["run_id"] = json!(trace.config.run_id);
        value["history"] = json!({"previous_actor_trace_index":trace.previous_actor_record[actor],
            "scope":"preceding recorded observations and transitions for this actor; trace-local history"});
        if trace.write_value(&value).is_err() {
            trace.stopped = Some("byte_limit_or_write_failure");
            trace.omitted += 1;
            eprintln!("gameplay trace stopped: byte limit or diagnostic write failure");
        } else {
            trace.previous_actor_record[actor] = Some(index);
            trace.written += 1;
        }
    }

    pub(crate) fn error(handle: &TraceHandle, reason: &'static str) {
        let Ok(mut trace) = handle.lock() else {
            return;
        };
        trace.stopped = Some(reason);
        trace.omitted += 1;
        eprintln!("gameplay trace stopped: {reason}");
    }
}

impl Drop for GameplayTraceV1 {
    fn drop(&mut self) {
        let footer = json!({"schema": SCHEMA_V1,"kind":"footer","written":self.written,
            "omitted":self.omitted,"complete":self.stopped.is_none(),"stop_reason":self.stopped,
            "bytes_before_footer":self.bytes});
        if let Ok(mut bytes) = serde_json::to_vec(&footer) {
            bytes.push(b'\n');
            if bytes.len() <= FOOTER_RESERVE && self.file.write_all(&bytes).is_ok() {
                let _ = self.file.flush();
            }
        }
    }
}

struct LimitedBuffer {
    bytes: Vec<u8>,
    cap: usize,
}
impl Write for LimitedBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len().saturating_add(buf.len()) > self.cap {
            return Err(std::io::Error::other("diagnostic record bound"));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(crate) struct PendingTrace {
    pub handle: TraceHandle,
    pub decision: FastActorDecisionV1,
    pub selected: u32,
    pub record: Value,
}

/// Search/snapshot clones never inherit an armed execution receipt.
#[derive(Default)]
pub(crate) struct TraceSlot(pub Mutex<Option<PendingTrace>>);
impl Clone for TraceSlot {
    fn clone(&self) -> Self {
        Self::default()
    }
}
impl Drop for TraceSlot {
    fn drop(&mut self) {
        if let Ok(slot) = self.0.get_mut() {
            if let Some(pending) = slot.take() {
                GameplayTraceV1::error(&pending.handle, "sampled_decision_not_executed");
            }
        }
    }
}

pub(crate) fn tensor_value(t: &NativeFlatDecisionTensorV2) -> Value {
    let bits = |values: &[f32]| values.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
    json!({"float_encoding":"IEEE754 binary32 u32 bits, flattened row-major",
        "state":bits(&t.state),"object_features":bits(&t.object_features),
        "object_card_ids":t.object_card_ids,"object_groups":t.object_groups,"object_node_ids":t.object_node_ids,
        "edge_features":bits(&t.edge_features),"edge_source_indices":t.edge_source_indices,
        "edge_target_indices":t.edge_target_indices,"action_features":bits(&t.action_features),
        "action_ref_features":bits(&t.action_ref_features),"action_ref_card_ids":t.action_ref_card_ids,
        "action_ref_action_indices":t.action_ref_action_indices,"action_ref_node_indices":t.action_ref_node_indices})
}

pub(crate) fn runtime_identity() -> Result<Value, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&exe).map_err(|e| e.to_string())?;
    Ok(
        json!({"executable_sha256":format!("{:x}",Sha256::digest(bytes)),
        "kernel_version":env!("CARGO_PKG_VERSION"),"target_os":std::env::consts::OS,
        "target_arch":std::env::consts::ARCH,"forward":"native scalar installed feature generation",
        "engine_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "tracked_tree_sha256":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
        "tracked_tree_contract":env!("MTG_KERNEL_BUILD_TRACKED_TREE_CONTRACT"),
        "build_git_clean":env!("MTG_KERNEL_BUILD_GIT_CLEAN"),
        "toolchain_file_sha256":format!("{:x}",Sha256::digest(include_bytes!("../../rust-toolchain.toml"))),
        "trace_feature":"gameplay-decision-trace-v1"}),
    )
}

pub(crate) fn actor_id(actor: PlayerSeatV1) -> crate::ids::PlayerId {
    match actor {
        PlayerSeatV1::P0 => crate::ids::PlayerId::P0,
        PlayerSeatV1::P1 => crate::ids::PlayerId::P1,
    }
}
