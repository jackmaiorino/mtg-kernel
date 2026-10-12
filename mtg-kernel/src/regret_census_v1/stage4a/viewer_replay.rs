//! Read-only reconstruction of retained real-game actions. No policy or search runs.
use super::{
    bank::{source_session, BankSpec},
    tree,
};
use crate::rl::{PlayerSeatV1, TerminalClassificationV1};
use crate::rl_session::FastActorResponseV1;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

const SCHEMA: &str = "mtg-kernel-viewer-reconstruction/v1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    path: PathBuf,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    spec: Input,
    bank: Input,
    witness: Input,
    id: String,
    output: PathBuf,
    focal_checkpoint: String,
    opponent_checkpoint: String,
    original_runtime: String,
}

fn read(input: &Input) -> Result<Vec<u8>, String> {
    if !input.path.is_absolute() {
        return Err("Input paths must be absolute".into());
    }
    let bytes = std::fs::read(&input.path).map_err(|e| e.to_string())?;
    if bytes.len() > 64 * 1024 * 1024 || format!("{:x}", Sha256::digest(&bytes)) != input.sha256 {
        return Err("Input exceeds bound or differs from its recorded hash".into());
    }
    Ok(bytes)
}

fn row(bytes: &[u8], id: &str) -> Result<Value, String> {
    let mut found = None;
    for line in bytes.split(|b| *b == b'\n').filter(|line| !line.is_empty()) {
        let value: Value = serde_json::from_slice(line).map_err(|e| e.to_string())?;
        if value["id"] == id {
            if found.replace(value).is_some() {
                return Err("Duplicate source identity".into());
            }
        }
    }
    found.ok_or("Recorded source identity missing".into())
}

fn write(file: &mut std::fs::File, mut value: Value) -> Result<(), String> {
    value["schema"] = json!(SCHEMA);
    let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("Reconstruction row exceeds bound".into());
    }
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|e| e.to_string())
}

pub(crate) fn run(path: &Path) -> Result<(), String> {
    let config: Config = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if !config.output.is_absolute() {
        return Err("Output path must be absolute".into());
    }
    let spec: BankSpec = serde_json::from_slice(&read(&config.spec)?).map_err(|e| e.to_string())?;
    spec.validate()?;
    let source = spec
        .sources
        .iter()
        .find(|s| s.id == config.id)
        .ok_or("Source spec missing")?;
    if source.seat != 0 {
        return Err("This reconstruction supports a seat0 perspective".into());
    }
    let bank = row(&read(&config.bank)?, &config.id)?;
    let witness = row(&read(&config.witness)?, &config.id)?;
    let guide = &witness["arms"]["R"];
    if bank["usable"] != true
        || guide["valid"] != true
        || guide["end"] != "win"
        || witness["lineage"] != bank["lineage"]
        || witness["own"] != bank["own"]
        || witness["other"] != bank["other"]
        || witness["opp"] != bank["opp"]
        || witness["seat"] != bank["seat"]
        || bank["seed"] != source.seed
        || witness["anchor_step"] != bank["anchor"]["step"]
    {
        return Err("Recorded guide identity, completeness or outcome differs".into());
    }
    let prefix: Vec<u32> =
        serde_json::from_value(bank["source_prefix"].clone()).map_err(|e| e.to_string())?;
    let suffix: Vec<u32> =
        serde_json::from_value(guide["actions"].clone()).map_err(|e| e.to_string())?;
    if prefix.len() as u64
        != witness["anchor_step"]
            .as_u64()
            .ok_or("Anchor step missing")?
        || prefix.len() + suffix.len() > 100000
    {
        return Err("Recorded action count differs".into());
    }
    let decisions = guide["decisions"]
        .as_array()
        .ok_or("Guide decisions missing")?;
    let searches = guide["searches"]
        .as_array()
        .ok_or("Guide searches missing")?;
    if searches
        .iter()
        .any(|s| s["incomplete"] != false || !s["faults"].as_array().is_some_and(Vec::is_empty))
    {
        return Err("Search was incomplete or faulted".into());
    }
    let cards = crate::card_def::CARD_DEFS
        .iter()
        .enumerate()
        .map(|(i, c)| {
            (
                i.to_string(),
                json!({"name":c.name,"type":format!("{:?}",c.types)}),
            )
        })
        .collect::<serde_json::Map<String, Value>>();
    let runtime = crate::gameplay_trace_v1::runtime_identity()?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&config.output)
        .map_err(|e| e.to_string())?;
    write(
        &mut file,
        json!({"kind":"header","perspective":"p0","cards":cards,"runtime":runtime,
        "decks":[spec.decks[source.own].label,spec.decks[source.other].label],
        "focal_checkpoint":config.focal_checkpoint,"opponent_checkpoint":config.opponent_checkpoint,
        "original_runtime":config.original_runtime,"witness_sha256":config.witness.sha256,
        "bank_sha256":config.bank.sha256,"spec_sha256":config.spec.sha256,
        "source_id":config.id,"runtime_rules":spec.runtime_rules,"allocation":witness["allocation"]}),
    )?;
    let mut session = source_session(&spec, source)?;
    let mut seen_decisions = 0;
    let mut seen_searches = 0;
    for (step, &action) in prefix.iter().chain(&suffix).enumerate() {
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            return Err("Early terminal during reconstruction".into());
        };
        if d.step != step as u64 || action >= d.legal_action_count {
            return Err("Recorded action is not legal at its step".into());
        }
        if step == prefix.len() {
            let (obs, menu) = super::replay_key(&session, d)?;
            if bank["anchor"]["obs_hash"] != obs
                || bank["anchor"]["menu_hash"] != menu
                || bank["anchor"]["state_hash"]
                    != format!("{:016x}", session.diagnostic_state_hash())
                || bank["anchor"]["physical_decision_id"] != d.physical_decision_id
            {
                return Err(
                    "Anchor reconstruction differs from retained observation, menu or state".into(),
                );
            }
        }
        let before = session.viewer_observation_v1(PlayerSeatV1::P0)?;
        let (observation, actions) = session
            .gameplay_trace_visible_v1(d, true)
            .map_err(|e| format!("{e:?}"))?;
        let labels = crate::human_bo3_v1::gameplay_trace_labels_v1(&observation, &actions)?;
        let mut evidence = Value::Null;
        if let Some(record) = decisions.iter().find(|r| r["step"] == d.step) {
            if d.acting_player != PlayerSeatV1::P0
                || record["physical_decision_id"] != d.physical_decision_id
                || record["substep"] != d.substep_index
                || record["k"] != d.legal_action_count
            {
                return Err("Recorded decision key or menu count differs".into());
            }
            let index = record["search"].as_u64().ok_or("Search index missing")? as usize;
            let search = searches.get(index).ok_or("Unknown search reference")?;
            evidence = json!({"chooser":record["chooser"],"root_step":search["step"],
                "tree_hash":search["tree"]["hash"],"witness_sha256":config.witness.sha256,
                "spy_self_chosen":record["spy_self_chosen"],"dr_giant_chosen":record["dr_giant_chosen"]});
            let semantics = session
                .diagnostic_current_action_semantics()
                .ok_or("Action semantics missing")?;
            let (_, own, giant) = super::labels::classify(
                &semantics[action as usize],
                session.game_state(),
                crate::ids::PlayerId::P0,
                &super::labels::spy_defs(),
            );
            if record["spy_self_chosen"] != own || record["dr_giant_chosen"] != giant {
                return Err("Recorded combo action differs from executed semantics".into());
            }
            seen_decisions += 1;
        }
        let mut search = Value::Null;
        if let Some(record) = searches.iter().find(|r| r["step"] == d.step) {
            if d.acting_player != PlayerSeatV1::P0 {
                return Err("Search root has a different actor".into());
            }
            let canon = tree::canon(&session, d).map_err(|e| format!("{e:?}"))?;
            let order: Vec<u32> = canon
                .edges
                .iter()
                .map(|e| tree::live_index(&canon, e).map_err(|e| format!("{e:?}")))
                .collect::<Result<_, _>>()?;
            let root = &record["tree"]["root"];
            let n: Vec<u64> =
                serde_json::from_value(root["n"].clone()).map_err(|e| e.to_string())?;
            let wins: Vec<u64> =
                serde_json::from_value(root["wins"].clone()).map_err(|e| e.to_string())?;
            let selected = root["frozen_choice"]
                .as_u64()
                .ok_or("Recorded root choice missing")? as usize;
            if order.len() != n.len()
                || n.len() != wins.len()
                || order.get(selected) != Some(&action)
                || n.iter().sum::<u64>()
                    != record["completed_natural"]
                        .as_u64()
                        .ok_or("Search count missing")?
                || wins.iter().zip(&n).any(|(w, n)| w > n)
            {
                return Err("Recorded search root mapping differs".into());
            }
            search = json!({"record":record,"live_order":order});
            seen_searches += 1;
        }
        session
            .step(d.episode_id, d.step, action)
            .map_err(|e| format!("{e:?}"))?;
        let after = session.viewer_observation_v1(PlayerSeatV1::P0)?;
        let terminal = match session.current_response() {
            FastActorResponseV1::Terminal(t) => json!(t),
            _ => Value::Null,
        };
        write(
            &mut file,
            json!({"kind":"decision","decision":d,"before":before,"after":after,
            "actions":actions,"labels":labels,"selected":action,"evidence":evidence,"search":search,"terminal":terminal}),
        )?;
    }
    let FastActorResponseV1::Terminal(t) = session.current_response() else {
        return Err("Recorded actions do not reach a terminal".into());
    };
    if t.terminal_classification != TerminalClassificationV1::Natural
        || t.winner != Some(PlayerSeatV1::P0)
        || seen_decisions != decisions.len()
        || seen_searches != searches.len()
    {
        return Err("Reconstructed terminal or evidence coverage differs".into());
    }
    write(
        &mut file,
        json!({"kind":"footer","written":prefix.len()+suffix.len(),"complete":true,
        "anchor_verified":true,"decision_keys_verified":seen_decisions,"search_roots_verified":seen_searches,"terminal":t}),
    )?;
    file.flush().map_err(|e| e.to_string())
}
