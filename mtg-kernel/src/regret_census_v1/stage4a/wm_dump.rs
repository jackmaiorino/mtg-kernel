//! `s4a-wm-dump`: plain development games (as `s4a-corpus`) with the exact
//! game facts at every decision, for the world-model accuracy probe (collab
//! LANES/world-model-probe-20261020). Measurement only: no search, no
//! training. Rows carry full engine truth, both hands included; the offline
//! probe applies the focal player's visibility itself.
use super::play::acting;
use super::{drive, game_setup, write_line, CensusConfigV1, Shared};
use crate::card_def::CardType;
use crate::ids::{ObjectId, PlayerId};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::runtime_decks::RUNTIME_DECKS;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::state::GameState;
use serde_json::{json, Value};
use std::sync::Mutex;

fn card(state: &GameState, id: ObjectId) -> u16 {
    state.objects.get(id).card_def
}

/// Effective type (transformed, bestowed, animated and face-down objects
/// included), as the engine itself answers it.
fn has_type(state: &GameState, id: ObjectId, t: CardType) -> bool {
    crate::engine::object_has_type(state, id, t)
}

/// Per-seat facts and zone contents. Battlefield entries are
/// `[card, tapped, power, toughness]` (power and toughness 0 for
/// non-creatures).
fn seat(state: &GameState, p: usize) -> Value {
    let ps = &state.players[p];
    let (mut creatures, mut lands, mut power, mut toughness, mut untapped_lands) = (0, 0, 0, 0, 0);
    let mut bf = Vec::with_capacity(ps.battlefield.len());
    for &id in &ps.battlefield {
        let o = state.objects.get(id);
        let (mut pw, mut tg) = (0, 0);
        if has_type(state, id, CardType::Creature) {
            pw = crate::engine::effective_power(state, id);
            tg = crate::engine::effective_toughness(state, id);
            creatures += 1;
            power += pw;
            toughness += tg;
        }
        if has_type(state, id, CardType::Land) {
            lands += 1;
            untapped_lands += i32::from(!o.tapped);
        }
        bf.push(json!([o.card_def, o.tapped, pw, tg]));
    }
    let cards = |ids: &[ObjectId]| ids.iter().map(|&i| card(state, i)).collect::<Vec<_>>();
    json!({"life":ps.life,"hand":ps.hand.len(),"library":ps.library.len(),
        "graveyard":ps.graveyard.len(),"creatures":creatures,"lands":lands,
        "untapped_lands":untapped_lands,"power":power,"toughness":toughness,
        "bf":bf,"gy":cards(&ps.graveyard),"hand_cards":cards(&ps.hand)})
}

pub(super) fn facts(state: &GameState) -> Value {
    let exile: Vec<[u16; 2]> = state
        .exile
        .iter()
        .map(|&id| [card(state, id), u16::from(state.objects.get(id).owner.0)])
        .collect();
    let stack: Vec<[u16; 2]> = state
        .stack
        .iter()
        .map(|it| [card(state, it.source), u16::from(it.controller.0)])
        .collect();
    json!({"turn":state.turn,"active":state.active_player.0,"phase":format!("{:?}", state.step),
        "seats":[seat(state, 0), seat(state, 1)],"exile":exile,"stack":stack})
}

/// The chosen action: its kind, every card it names in serialization order,
/// and the full semantic (choices, roles, target players, indices) so the
/// probe can apply per-field visibility itself.
fn action(s: &FastActorSessionV1, a: u32) -> Result<Value, String> {
    let sem = s
        .diagnostic_current_action_semantics()
        .ok_or("missing semantics")?;
    let v = serde_json::to_value(&sem[a as usize]).map_err(|e| e.to_string())?;
    fn walk(v: &Value, out: &mut Vec<u64>) {
        match v {
            Value::Object(m) => {
                if let Some(c) = m.get("card_db_id").and_then(Value::as_u64) {
                    out.push(c);
                }
                m.values().for_each(|x| walk(x, out));
            }
            Value::Array(xs) => xs.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut cards = Vec::new();
    walk(&v, &mut cards);
    Ok(json!({"kind":v["action_kind"],"cards":cards,"semantic":v}))
}

pub(super) fn wm_dump_game(
    cfg: &CensusConfigV1,
    shared: &Shared,
    focal: &mut FrozenPlayPolicyV1,
    opps: &mut [FrozenPlayPolicyV1],
    game: u64,
    sink: &Mutex<std::fs::File>,
) -> Result<(), String> {
    let setup = game_setup(cfg, shared.labels.len(), game);
    let mut steps = Vec::new();
    let (s, finished) = drive(&setup, focal, &mut opps[setup.model], |s, d, a| {
        steps.push(
            json!({"step":d.step,"actor":u8::from(acting(d) == PlayerId(1)),
            "legal":d.legal_action_count,"choice":a,"action":action(s, a)?,
            "facts":facts(s.game_state())}),
        );
        Ok(false)
    })?;
    let end = match s.current_response() {
        FastActorResponseV1::Terminal(t) => {
            json!({"classification":format!("{:?}", t.terminal_classification),
            "winner":t.winner.map(|w| u8::from(w == crate::rl::PlayerSeatV1::P1))})
        }
        FastActorResponseV1::Decision(_) => Value::Null,
    };
    let row = json!({"kind":"s4a_wm_game","game":game,"seed":setup.seed,
        "decks":[RUNTIME_DECKS[setup.decks[0]].id,RUNTIME_DECKS[setup.decks[1]].id],
        "focal_seat":setup.focal,"opp_model":shared.labels[setup.model],"starting_player":setup.starting,
        "finished":finished,"end":end,"final":facts(s.game_state()),"steps":steps});
    write_line(sink, &row)
}
