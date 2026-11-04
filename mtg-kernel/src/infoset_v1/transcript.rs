//! The strict view for AC1, harness side (lead rulings 4 and 5 and the
//! 2026-11-04 clarification).
//!
//! V's transcript is what the entrant's seat receives through the Spellbench
//! adapter (`kernel_engine_v2.py` in front of the bridge): per-viewer object
//! ids re-minted on every zone change, the projected observation, the menu,
//! `x_kernel_flat_v4`, `x_public_history_v1` and the tracker's knowledge. The
//! harness here reaches only the engine, so it writes, at every decision of
//! each world, the bridge's own wire response (built by the bridge's code
//! from a raw session at the copy's state, [`BridgeMirrorV1`]), the raw
//! candidate each world answered, and at V's decisions P's and each mutant's
//! delivered outcome (as digests) plus V's spans. The checker
//! (`ac1_strict_view.py`, collab LANES/lead/w1/composition/tools) runs the
//! adapter code the entrant runs over those responses, renames each
//! bot-facing id by its first appearance in V's transcript, truncates a pair
//! at the first difference and tallies P and the mutants there.
//!
//! Dumps are written only when `W1_STRICT_DUMP_DIR` names a directory.

use crate::agent_bridge_v1::BridgeMirrorV1;
use crate::ids::PlayerId;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::state::Zone;
use serde_json::{json, Value};
use std::io::Write as _;

fn zone_name(z: Zone) -> &'static str {
    match z {
        Zone::Library => "library",
        Zone::Hand => "hand",
        Zone::Battlefield => "battlefield",
        Zone::Graveyard => "graveyard",
        Zone::Stack => "stack",
        Zone::Exile => "exile",
        Zone::Command => "command",
    }
}

/// One world's bridge mirror plus what V's spans need.
#[derive(Clone)]
pub(crate) struct WorldV1 {
    pub viewer: PlayerId,
    mirror: BridgeMirrorV1,
    /// The projection's stream length at each V decision so far.
    items_at: Vec<usize>,
    /// Raw candidate executed by each of the copy's rows at the last decision.
    pub rows: Vec<u32>,
}

impl WorldV1 {
    pub(crate) fn new(viewer: PlayerId) -> Self {
        Self {
            viewer,
            mirror: BridgeMirrorV1::new(),
            items_at: Vec::new(),
            rows: Vec::new(),
        }
    }

    /// At every decision of either seat, in order: the bridge response, and
    /// at V's decisions V's spans.
    pub(crate) fn decision(&mut self, s: &FastActorSessionV1) -> Value {
        let mut out = match self.mirror.response(s) {
            Ok((r, rows)) => {
                self.rows = rows;
                json!({"r": r})
            }
            Err(e) => {
                self.rows = Vec::new();
                json!({"error": e})
            }
        };
        if let FastActorResponseV1::Decision(d) = s.current_response() {
            if super::harness::seat(d.acting_player) == self.viewer {
                out["spans"] = self.spans(s);
            }
        }
        out
    }

    /// The raw candidate for row `row` at the last decision.
    pub(crate) fn raw_candidate(&self, row: usize) -> Value {
        self.rows.get(row).map_or(Value::Null, |c| json!(c))
    }

    /// V's spans without object identity (no native id, no name): for each
    /// span, the V decision at which it started (and ended), its position in
    /// the start record and the zone it ended in, as a sorted list.
    fn spans(&mut self, s: &FastActorSessionV1) -> Value {
        let state = s.game_state();
        let p = super::projection::project_v1(state, state.observation_journal.notes(), self.viewer);
        self.items_at.push(p.items);
        let ordinal = |item: usize| self.items_at.partition_point(|&n| n <= item);
        let mut spans: Vec<String> = Vec::new();
        for o in &p.objects {
            for sp in &o.spans {
                spans.push(
                    json!([
                        ordinal(sp.start_item),
                        sp.start_pos,
                        sp.end.map(|e| ordinal(e.item)),
                        sp.end.map(|e| zone_name(e.zone))
                    ])
                    .to_string(),
                );
            }
        }
        spans.sort();
        json!(spans)
    }
}

/// A gzip JSONL dump, or nothing when `W1_STRICT_DUMP_DIR` is unset.
pub(crate) struct DumpV1 {
    out: Option<flate2::write::GzEncoder<std::fs::File>>,
}

impl DumpV1 {
    pub(crate) fn enabled() -> bool {
        std::env::var_os("W1_STRICT_DUMP_DIR").is_some()
    }

    pub(crate) fn open(name: &str, header: Value) -> Self {
        let Some(dir) = std::env::var_os("W1_STRICT_DUMP_DIR") else {
            return Self { out: None };
        };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).expect("dump directory");
        let safe: String = name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
            .collect();
        let file = std::fs::File::create(dir.join(format!("{safe}.jsonl.gz"))).expect("dump file");
        let mut d = Self {
            out: Some(flate2::write::GzEncoder::new(
                file,
                flate2::Compression::fast(),
            )),
        };
        d.line(&header);
        d
    }

    pub(crate) fn line(&mut self, v: &Value) {
        if let Some(out) = self.out.as_mut() {
            serde_json::to_writer(&mut *out, v).expect("dump line");
            out.write_all(b"\n").expect("dump line");
        }
    }

    pub(crate) fn finish(mut self, end: &str) {
        self.line(&json!({"end": end}));
        if let Some(out) = self.out.take() {
            out.finish().expect("dump finish");
        }
    }
}

/// A comparable digest of a delivered outcome.
pub(crate) fn digest(o: &Result<Vec<u8>, String>) -> String {
    use sha2::Digest as _;
    match o {
        Ok(b) => format!("{:x}", sha2::Sha256::digest(b))[..24].to_string(),
        Err(e) => format!("Err({e})"),
    }
}
