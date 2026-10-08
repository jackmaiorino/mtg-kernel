//! Paired pilot comparison (diagnostic only): the candidate policy pilots
//! `cfg.pilot_deck` against the reference policy (REF_SOURCE) on each other
//! deck, both seats and both starting players. Game seeds depend only on the
//! game index, so runs with different candidates are paired. Logs the
//! outcome and how the pilot used chosen cards, including the two Spy combo
//! targets (forced ones too) under `combo|...` keys. Logging never changes
//! play.

use super::{mix, named, seat_index, MAX_PHYSICAL};
use crate::ids::PlayerId;
use crate::paired_bo1_harness_v1::paired_policy_seeds_v1;
use crate::rl::TerminalClassificationV1;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::runtime_decks::RUNTIME_DECKS;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde_json::json;
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::Mutex;

const TRACKED: &[&str] = &[
    "Prismatic Strands",
    "Basilisk Gate",
    "Heap Gate",
    "Citadel Gate",
    "Sea Gate",
    "Balustrade Spy",
    "Dread Return",
    "Lotleth Giant",
];

pub(super) fn run_duel_game(
    cfg: &super::CensusConfigV1,
    game: u64,
    cand: &mut FrozenPlayPolicyV1,
    reference: &mut FrozenPlayPolicyV1,
    sink: &Mutex<std::fs::File>,
) -> Result<(), String> {
    let n = cfg.decks.len() as u64;
    let pilot = (game % 2) as usize;
    let opp_deck = cfg.decks[((game / 2) % n) as usize];
    let starting = ((game / (2 * n)) % 2) as u8;
    let seed = mix(cfg.base_seed ^ mix(game));
    let mut deck_ix = [opp_deck; 2];
    deck_ix[pilot] = cfg.pilot_deck;
    let decks = [&RUNTIME_DECKS[deck_ix[0]], &RUNTIME_DECKS[deck_ix[1]]];
    let mut session =
        FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
            1,
            seed,
            MAX_PHYSICAL,
            MAX_PHYSICAL * 128,
            [decks[0].id.to_owned(), decks[1].id.to_owned()],
            [decks[0].card_ids.to_vec(), decks[1].card_ids.to_vec()],
            PlayerId(starting),
        )
        .map_err(|e| format!("{e:?}"))?;
    let seeds = paired_policy_seeds_v1(seed);
    cand.reset_sampling_v1(seeds);
    reference.reset_sampling_v1(seeds);
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut bump = |k: String| *counts.entry(k).or_default() += 1;
    // Turn-level combat bookkeeping for the pilot.
    let mut attacked_turns: Vec<u32> = Vec::new();
    let mut pump_turns: Vec<u32> = Vec::new();
    let mut pilot_decisions = 0u64;
    let (score, turns) = loop {
        match session.current_response() {
            FastActorResponseV1::Terminal(t) => {
                let natural = t.terminal_classification == TerminalClassificationV1::Natural;
                break (
                    match t.winner {
                        Some(w) if natural => f64::from(u8::from(seat_index(w) == pilot)),
                        _ => 0.5,
                    },
                    session.game_state().turn,
                );
            }
            FastActorResponseV1::Decision(d) => {
                let actor = seat_index(d.acting_player);
                let a = if actor == pilot {
                    cand.select_fast_session_v1(&session)?
                } else {
                    reference.select_fast_session_v1(&session)?
                };
                if actor == pilot && d.legal_action_count == 1 {
                    // A forced combo target still counts as reached.
                    let sem = session
                        .diagnostic_current_action_semantics()
                        .ok_or("semantics")?;
                    let v = named(serde_json::to_value(&sem[a as usize]).unwrap_or_default());
                    if let Some(key) = combo_target(&v, pilot) {
                        bump(format!("{key}|forced"));
                    }
                }
                if actor == pilot && d.legal_action_count >= 2 {
                    pilot_decisions += 1;
                    let st = session.game_state();
                    let sem = session
                        .diagnostic_current_action_semantics()
                        .ok_or("semantics")?;
                    let v = named(serde_json::to_value(&sem[a as usize]).unwrap_or_default());
                    if let Some(key) = combo_target(&v, pilot) {
                        bump(key);
                    }
                    let src = v["source"].as_str().unwrap_or("");
                    let kind = v["action_kind"].as_str().unwrap_or("");
                    let side = if st.active_player.0 as usize == pilot {
                        "own"
                    } else {
                        "opp"
                    };
                    let step = format!("{:?}", st.step);
                    if TRACKED.contains(&src) && kind != "activate_mana_ability" {
                        let extra = v["color"]
                            .as_str()
                            .map(|c| format!("|{c}"))
                            .unwrap_or_default();
                        bump(format!("{src}|{kind}|{step}|{side}{extra}"));
                    }
                    if src == "Basilisk Gate" && kind == "activate_ability" {
                        pump_turns.push(st.turn);
                    }
                    if kind == "choose_attacker_inclusion"
                        && v["include"] == json!(true)
                        && attacked_turns.last() != Some(&st.turn)
                    {
                        attacked_turns.push(st.turn);
                    }
                    if kind == "cast_spell" {
                        bump(format!("cast|{src}|{side}"));
                    }
                }
                session
                    .step(d.episode_id, d.step, a)
                    .map_err(|e| format!("{e:?}"))?;
            }
        }
    };
    let st = session.game_state();
    let row = json!({"kind":"duel","game":game,"pilot_seat":pilot,"pilot_deck":decks[pilot].id,
        "opp_deck":RUNTIME_DECKS[opp_deck].id,"starting_player":starting,"score":score,"turns":turns,
        "final_life":[st.players[pilot].life, st.players[1-pilot].life],
        "pilot_decisions":pilot_decisions,"pump_turns":pump_turns,"attacked_turns":attacked_turns,
        "counts":counts});
    let mut f = sink.lock().map_err(|_| "sink poisoned")?;
    writeln!(f, "{row}").map_err(|e| e.to_string())?;
    Ok(())
}

/// Key for a chosen Spy combo target: Balustrade Spy aimed at its controller
/// (`self`) or the opponent, and Dread Return's target card by name.
fn combo_target(v: &serde_json::Value, pilot: usize) -> Option<String> {
    if v["action_kind"] != "choose_target" {
        return None;
    }
    let target = &v["target"];
    match v["source"].as_str()? {
        "Balustrade Spy" => {
            let side = if target["player"] == format!("p{pilot}") {
                "self"
            } else {
                "opp"
            };
            Some(format!("combo|spy_target|{side}"))
        }
        "Dread Return" => Some(format!(
            "combo|dread_return_target|{}",
            target["object"].as_str().unwrap_or("?")
        )),
        _ => None,
    }
}
