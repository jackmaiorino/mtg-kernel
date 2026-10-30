//! Position dumps for human review of reach roots (mode `s4a-review-export`).
//! Presentation only: nothing that chooses, samples, trains or selects reads
//! these rows, and they never become goal states.
//!
//! Each line of `REVIEW_ROOTS` is a reach root (`tools/reach` planner schema:
//! `root_id`, `index`, `job`, `roles` with `certified_index`). The job is
//! replayed exactly as `s4a-win-check` replays it, checked against its record
//! at every decision, and the row carries the full board (hidden zones
//! included; the viewer hides them by default) at the root and at the
//! certified decision, every recorded decision in between, and the legal
//! actions at both ends.

use super::labels::spy_defs;
use super::play::{acting, apply, decision, Meter};
use super::replay_labels::LabelJob;
use crate::card_def::CardType;
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, Zone};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Role {
    role: String,
    stratum: String,
    certified_index: Option<usize>,
    opportunity_turn: Option<u32>,
}

#[derive(Deserialize)]
struct Root {
    root_id: String,
    index: usize,
    job: LabelJob,
    roles: Vec<Role>,
}

fn card(state: &GameState, id: ObjectId) -> Value {
    let o = state.objects.get(id);
    let mut v = json!({"id": id.0, "db": o.card_def, "name": crate::rl::card_name(o.card_def)});
    if o.zone == Zone::Battlefield {
        v["tapped"] = json!(o.tapped);
        v["sick"] = json!(o.summoning_sick);
        if o.damage > 0 {
            v["damage"] = json!(o.damage);
        }
        if o.counters.plus1_plus1 != 0 {
            v["plus1"] = json!(o.counters.plus1_plus1);
        }
        if crate::engine::object_has_type(state, id, CardType::Creature) {
            v["pt"] = json!([
                crate::engine::effective_power(state, id),
                crate::engine::effective_toughness(state, id)
            ]);
        }
    }
    v
}

/// The whole board. Library order and both hands are hidden information;
/// the reviewer's view decides what to show.
fn board(s: &crate::rl_session::FastActorSessionV1) -> Value {
    let state = s.game_state();
    let cards = |ids: &[ObjectId]| ids.iter().map(|&id| card(state, id)).collect::<Vec<_>>();
    let players = (0..2)
        .map(|p| {
            let ps = &state.players[p];
            let exile: Vec<Value> = state
                .objects
                .iter()
                .filter(|(_, o)| o.zone == Zone::Exile && o.owner == PlayerId(p as u8))
                .map(|(id, _)| card(state, id))
                .collect();
            json!({"life": ps.life, "hand": cards(&ps.hand), "library": cards(&ps.library),
                "battlefield": cards(&ps.battlefield), "graveyard": cards(&ps.graveyard),
                "exile": exile, "mana_pool": ps.mana_pool})
        })
        .collect::<Vec<_>>();
    let stack = state
        .stack
        .iter()
        .map(|item| {
            json!({"source": card(state, item.source), "controller": item.controller.0,
                "kind": format!("{:?}", item.kind)})
        })
        .collect::<Vec<_>>();
    json!({"turn": state.turn, "active": state.active_player.0, "priority": state.priority_player.0,
        "step": format!("{:?}", state.step), "players": players, "stack": stack})
}

fn legal(s: &crate::rl_session::FastActorSessionV1) -> Vec<String> {
    s.diagnostic_current_action_semantics()
        .unwrap_or_default()
        .iter()
        .map(|a| format!("{a:?}"))
        .collect()
}

fn export_root(root: &Root) -> Result<Value, String> {
    let job = &root.job;
    let role = root
        .roles
        .iter()
        .find(|r| r.certified_index.is_some())
        .or(root.roles.first())
        .ok_or("root has no role")?;
    let certified = role.certified_index.unwrap_or(root.index);
    if certified < root.index || certified >= job.decisions.len() {
        return Err(format!("certified index {certified} is outside the game"));
    }
    let mut s = crate::learned_bo3_v1::replay_game_session_v1(
        &job.config,
        job.uses_v3,
        job.game_index,
        job.environment_seed,
        job.mainboards.clone(),
        PlayerId(job.starting_player),
    )?;
    let (focal, defs, mut meter) = (PlayerId(job.focal), spy_defs(), Meter::new(u64::MAX));
    let mut at_root = Value::Null;
    let mut path = Vec::new();
    for (i, r) in job.decisions.iter().enumerate().take(certified + 1) {
        let d = decision(&s).ok_or_else(|| format!("replay ended before decision {i}"))?;
        if d.step != r.step
            || d.physical_decision_id != r.physical_decision_id
            || acting(&d).0 != r.actor
            || d.legal_action_count != r.legal_action_count
        {
            return Err(format!("replay differs from the record at decision {i}"));
        }
        if i == root.index {
            at_root = json!({"board": board(&s), "legal": legal(&s), "actor": r.actor});
        }
        if i == certified {
            let row = json!({"board": board(&s), "legal": legal(&s), "actor": r.actor,
                "selected": r.selected});
            // Every object the game has created so far (the arena keeps them), so
            // actions on cards seen only between the two boards still have names.
            let names: std::collections::BTreeMap<u16, String> = s
                .game_state()
                .objects
                .iter()
                .map(|(_, o)| (o.card_def, crate::rl::card_name(o.card_def)))
                .collect();
            return Ok(
                json!({"kind": "review", "root_id": root.root_id, "game_id": job.id,
                "role": role.role, "stratum": role.stratum, "opportunity_turn": role.opportunity_turn,
                "index": root.index, "certified_index": certified, "focal": job.focal,
                "deck_ids": job.config.deck_ids, "winner": job.winner,
                "root": at_root, "certified": row, "path": path, "names": names}),
            );
        }
        if i >= root.index {
            let chosen = s
                .diagnostic_current_action_semantics()
                .and_then(|v| v.get(r.selected as usize).map(|a| format!("{a:?}")));
            let state = s.game_state();
            path.push(json!({"index": i, "actor": r.actor, "turn": state.turn,
                "step": format!("{:?}", state.step), "legal": r.legal_action_count,
                "selected": r.selected, "action": chosen}));
        }
        apply(&mut s, &d, r.selected, &mut meter, focal, None, &defs)
            .map_err(|e| format!("decision {i}: {e:?}"))?;
    }
    Err("certified decision not reached".into())
}

pub(super) fn run(cfg: &super::super::CensusConfigV1) -> Result<(), String> {
    use std::io::Write;
    let path = std::env::var("REVIEW_ROOTS").map_err(|_| "REVIEW_ROOTS is required")?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let mut out =
        std::io::BufWriter::new(std::fs::File::create(&cfg.out).map_err(|e| e.to_string())?);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let root: Root = serde_json::from_str(line).map_err(|e| format!("{path}: {e}"))?;
        let row = export_root(&root)
            .unwrap_or_else(|e| json!({"kind": "error", "root_id": root.root_id, "error": e}));
        writeln!(out, "{row}").map_err(|e| e.to_string())?;
    }
    out.flush().map_err(|e| e.to_string())
}
