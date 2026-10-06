//! Offline regret census for a frozen play policy.
//!
//! Plays the policy against itself over the runtime deck catalog. At a random
//! sample of multi-action decisions it estimates every candidate action's win
//! probability by continuing the game with the same policy from determinized
//! copies of the actor's information set, using common random numbers across
//! candidates. Diagnostic only: no training, store records or gates.

use crate::expanded_deck_training_v1::{load_expanded_inference_v1, ExpandedModelSourceV1};
use crate::ids::PlayerId;
use crate::paired_bo1_harness_v1::paired_policy_seeds_v1;
use crate::rl::{PlayerSeatV1, TerminalClassificationV1};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::runtime_decks::RUNTIME_DECKS;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::state::SplitMix64;
use serde_json::json;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const MAX_PHYSICAL: u64 = 4096;

#[derive(Clone, Debug)]
pub struct CensusConfigV1 {
    pub source: String,
    pub out: String,
    pub first_game: u64,
    pub games: u64,
    pub base_seed: u64,
    pub root_prob: f64,
    pub rollouts: u32,
    pub max_actions: usize,
    pub workers: usize,
    pub decks: Vec<usize>,
}

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn seat_index(seat: PlayerSeatV1) -> usize {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

fn variant(debug: &str) -> String {
    debug
        .split(|c: char| c == ' ' || c == '{' || c == '(')
        .next()
        .unwrap_or("")
        .to_owned()
}

/// Play to the end with `policy` for both seats; the score for `seat`
/// (1 win, 0.5 draw or non-natural, 0 loss) and the turn reached.
fn play_out(
    session: &mut FastActorSessionV1,
    policy: &mut FrozenPlayPolicyV1,
    seat: usize,
) -> Result<(f64, u32, bool), String> {
    loop {
        match session.current_response() {
            FastActorResponseV1::Terminal(t) => {
                let natural = t.terminal_classification == TerminalClassificationV1::Natural;
                let score = match t.winner {
                    Some(w) if natural => {
                        if seat_index(w) == seat {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    _ => 0.5,
                };
                return Ok((score, session.game_state().turn, natural));
            }
            FastActorResponseV1::Decision(d) => {
                let a = policy.select_fast_session_v1(session)?;
                session
                    .step(d.episode_id, d.step, a)
                    .map_err(|e| format!("{e:?}"))?;
            }
        }
    }
}

fn softmax(logits: &[f32]) -> Vec<f64> {
    let m = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max) as f64;
    let e: Vec<f64> = logits.iter().map(|&x| ((x as f64) - m).exp()).collect();
    let s: f64 = e.iter().sum();
    e.into_iter().map(|x| x / s).collect()
}

fn run_game(
    cfg: &CensusConfigV1,
    game: u64,
    base: &mut FrozenPlayPolicyV1,
    roll: &mut FrozenPlayPolicyV1,
    sink: &Mutex<std::fs::File>,
) -> Result<(), String> {
    let n = cfg.decks.len() as u64;
    let d0 = cfg.decks[(game % n) as usize];
    let d1 = cfg.decks[((game / n) % n) as usize];
    let starting = ((game / (n * n)) % 2) as u8;
    let seed = mix(cfg.base_seed ^ mix(game));
    let decks = [&RUNTIME_DECKS[d0], &RUNTIME_DECKS[d1]];
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
    base.reset_sampling_v1(paired_policy_seeds_v1(seed));
    let mut root_rng = SplitMix64::seed(mix(seed ^ 0x524F_4F54));
    let mut roots = Vec::new();
    let mut multi = [0u32; 2];
    let mut decisions = [0u32; 2];
    let (winner, natural, turns) = loop {
        match session.current_response() {
            FastActorResponseV1::Terminal(t) => {
                let natural = t.terminal_classification == TerminalClassificationV1::Natural;
                break (t.winner.map(seat_index), natural, session.game_state().turn);
            }
            FastActorResponseV1::Decision(d) => {
                let actor = seat_index(d.acting_player);
                decisions[actor] += 1;
                let k = d.legal_action_count as usize;
                if k >= 2 {
                    multi[actor] += 1;
                    let draw = (root_rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
                    if draw < cfg.root_prob {
                        roots.push(evaluate_root(cfg, &session, roll, seed, roots.len() as u64)?);
                    }
                }
                let a = base.select_fast_session_v1(&session)?;
                if let Some(last) = roots.last_mut() {
                    if last["step"] == json!(d.step) {
                        last["base_chosen"] = json!(a);
                    }
                }
                session
                    .step(d.episode_id, d.step, a)
                    .map_err(|e| format!("{e:?}"))?;
            }
        }
    };
    let mut out = String::new();
    let game_row = json!({"kind":"game","game":game,"seed":seed,"decks":[decks[0].id,decks[1].id],
        "starting_player":starting,"winner":winner,"natural":natural,"turns":turns,
        "decisions":decisions,"multi_action_decisions":multi,"roots":roots.len()});
    out.push_str(&game_row.to_string());
    out.push('\n');
    for mut r in roots {
        r["game"] = json!(game);
        r["decks"] = json!([decks[0].id, decks[1].id]);
        r["starting_player"] = json!(starting);
        r["base_winner"] = json!(winner);
        out.push_str(&r.to_string());
        out.push('\n');
    }
    let mut f = sink.lock().map_err(|_| "sink poisoned")?;
    f.write_all(out.as_bytes()).map_err(|e| e.to_string())?;
    f.flush().map_err(|e| e.to_string())?;
    Ok(())
}

fn evaluate_root(
    cfg: &CensusConfigV1,
    session: &FastActorSessionV1,
    roll: &mut FrozenPlayPolicyV1,
    game_seed: u64,
    ordinal: u64,
) -> Result<serde_json::Value, String> {
    let FastActorResponseV1::Decision(d) = session.current_response() else {
        return Err("root is terminal".into());
    };
    let actor = seat_index(d.acting_player);
    roll.reset_sampling_v1([1, 2]);
    let scores = roll.score_fast_session_v1(session)?;
    let probs = softmax(&scores.logits);
    let semantics = session
        .diagnostic_current_action_semantics()
        .ok_or("missing semantics")?;
    let k = probs.len();
    let mut order: Vec<usize> = (0..k).collect();
    order.sort_by(|&a, &b| probs[b].partial_cmp(&probs[a]).unwrap());
    let evaluated: Vec<usize> = order.into_iter().take(cfg.max_actions).collect();
    let state = session.game_state();
    let m = cfg.rollouts as usize;
    let mut wins = vec![vec![0f64; m]; evaluated.len()];
    let mut turns = vec![vec![0u32; m]; evaluated.len()];
    let mut non_natural = 0u32;
    let root_seed = mix(game_seed ^ mix(0xC0FFEE ^ (d.step << 16) ^ ordinal));
    for r in 0..m {
        let det = mix(root_seed ^ (r as u64 + 1));
        let pol = [mix(det ^ 0xA1), mix(det ^ 0xB2)];
        for (i, &a) in evaluated.iter().enumerate() {
            let mut s = session.census_redeterminized_clone_v1(det)?;
            s.step(d.episode_id, d.step, a as u32)
                .map_err(|e| format!("root step {a}: {e:?}"))?;
            roll.reset_sampling_v1(pol);
            let (score, turn, natural) = play_out(&mut s, roll, actor)?;
            if !natural {
                non_natural += 1;
            }
            wins[i][r] = score;
            turns[i][r] = turn;
        }
    }
    let kinds: Vec<String> = semantics.iter().map(|s| variant(&format!("{s:?}"))).collect();
    let detail: Vec<String> = evaluated
        .iter()
        .map(|&a| {
            let t = format!("{:?}", semantics[a]);
            t.chars().take(240).collect()
        })
        .collect();
    let opp = 1 - actor;
    Ok(json!({"kind":"root","step":d.step,"actor":actor,"turn":state.turn,
        "phase":format!("{:?}",state.step),"active":state.active_player.0,
        "life":[state.players[actor].life,state.players[opp].life],
        "hand":[state.players[actor].hand.len(),state.players[opp].hand.len()],
        "stack":state.stack.len(),"substep":[d.substep_index,d.substep_count],
        "k":k,"kinds":kinds,"probs":probs,"evaluated":evaluated,"detail":detail,
        "wins":wins,"rollout_turns":turns,"non_natural":non_natural,"base_chosen":null}))
}

pub fn run_v1(cfg: CensusConfigV1) -> Result<(), String> {
    let source: ExpandedModelSourceV1 = serde_json::from_slice(
        &std::fs::read(&cfg.source).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let (policy, identity) = load_expanded_inference_v1(&source)?;
    eprintln!("loaded {}", serde_json::to_string(&identity).unwrap_or_default());
    let sink = Mutex::new(
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&cfg.out)
            .map_err(|e| e.to_string())?,
    );
    let next = AtomicU64::new(cfg.first_game);
    let end = cfg.first_game + cfg.games;
    let started = std::time::Instant::now();
    let done = AtomicU64::new(0);
    std::thread::scope(|scope| -> Result<(), String> {
        let mut handles = Vec::new();
        for _ in 0..cfg.workers {
            let mut base = policy.fork_for_collection_v3()?;
            let mut roll = policy.fork_for_collection_v3()?;
            let (cfg, sink, next, done) = (&cfg, &sink, &next, &done);
            handles.push(scope.spawn(move || -> Result<(), String> {
                loop {
                    let g = next.fetch_add(1, Ordering::SeqCst);
                    if g >= end {
                        return Ok(());
                    }
                    if let Err(e) = run_game(cfg, g, &mut base, &mut roll, sink) {
                        eprintln!("game {g} failed: {e}");
                        let row = json!({"kind":"error","game":g,"error":e});
                        let mut f = sink.lock().map_err(|_| "sink poisoned")?;
                        writeln!(f, "{row}").map_err(|e| e.to_string())?;
                    }
                    let c = done.fetch_add(1, Ordering::SeqCst) + 1;
                    eprintln!("game {g} done ({c}) at {:.1}s", started.elapsed().as_secs_f64());
                }
            }));
        }
        for h in handles {
            h.join().map_err(|_| "worker panicked")??;
        }
        Ok(())
    })
}
