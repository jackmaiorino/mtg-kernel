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
use crate::human_opening_v1::HumanOpeningV1;
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
    pub mode: String,
    pub pilot_deck: usize,
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
    play_out_traced(session, policy, seat, None)
}

fn play_out_traced(
    session: &mut FastActorSessionV1,
    policy: &mut FrozenPlayPolicyV1,
    seat: usize,
    trace: Option<usize>,
) -> Result<(f64, u32, bool), String> {
    let mut traced = 0usize;
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
                if let Some(limit) = trace {
                    if traced < limit && d.legal_action_count > 1 {
                        traced += 1;
                        let st = session.game_state();
                        let sem = session
                            .diagnostic_current_action_semantics()
                            .map(|v| named(serde_json::to_value(&v[a as usize]).unwrap_or_default()).to_string())
                            .unwrap_or_default();
                        eprintln!(
                            "  t{} {:?} P{} life={:?} k={} -> {}",
                            st.turn,
                            st.step,
                            seat_index(d.acting_player),
                            [st.players[0].life, st.players[1].life],
                            d.legal_action_count,
                            sem.chars().take(200).collect::<String>()
                        );
                    }
                }
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
            let trace = std::env::var("CENSUS_TRACE")
                .ok()
                .and_then(|v| {
                    let (st, rr) = v.split_once(':')?;
                    (st.parse::<u64>().ok()? == d.step && rr.parse::<usize>().ok()? == r).then_some(400)
                });
            if trace.is_some() {
                eprintln!("TRACE step {} rollout {r} action {a}: {:?}", d.step, semantics[a]);
            }
            let (score, turn, natural) = play_out_traced(&mut s, roll, actor, trace)?;
            if trace.is_some() {
                eprintln!("  => score {score} turn {turn}");
            }
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


/// Rollout value of an opening for `observer`: continue with the policy until
/// the observer's first decision, then determinize the observer's information
/// set and play out. Returns one score per rollout.
fn opening_rollouts(
    session0: &FastActorSessionV1,
    roll: &mut FrozenPlayPolicyV1,
    observer: usize,
    seeds: &[u64],
) -> Result<Vec<f64>, String> {
    let mut scores = Vec::with_capacity(seeds.len());
    for &det in seeds {
        let mut s = session0.clone();
        roll.reset_sampling_v1([mix(det ^ 0xA1), mix(det ^ 0xB2)]);
        loop {
            match s.current_response() {
                FastActorResponseV1::Decision(d) if seat_index(d.acting_player) != observer => {
                    let a = roll.select_fast_session_v1(&s)?;
                    s.step(d.episode_id, d.step, a).map_err(|e| format!("{e:?}"))?;
                }
                _ => break,
            }
        }
        if matches!(s.current_response(), FastActorResponseV1::Decision(_)) {
            s = s.census_redeterminized_clone_v1(det)?;
        }
        let (score, _, _) = play_out(&mut s, roll, observer)?;
        scores.push(score);
    }
    Ok(scores)
}

fn hand_ids(opening: &HumanOpeningV1) -> Vec<u16> {
    opening.view().hand.iter().map(|c| c.card_id).collect()
}

fn run_mulligan_game(
    cfg: &CensusConfigV1,
    game: u64,
    roll: &mut FrozenPlayPolicyV1,
    sink: &Mutex<std::fs::File>,
) -> Result<(), String> {
    let n = cfg.decks.len() as u64;
    let d0 = cfg.decks[(game % n) as usize];
    let d1 = cfg.decks[((game / n) % n) as usize];
    let starting = ((game / (n * n)) % 2) as u8;
    let seed = mix(cfg.base_seed ^ mix(game));
    let decks = [&RUNTIME_DECKS[d0], &RUNTIME_DECKS[d1]];
    let ids = [decks[0].id.to_owned(), decks[1].id.to_owned()];
    let boards = [decks[0].card_ids.to_vec(), decks[1].card_ids.to_vec()];
    let new_opening = |observer: u8| {
        HumanOpeningV1::new(1, seed, MAX_PHYSICAL, MAX_PHYSICAL * 128, ids.clone(), boards.clone(),
            PlayerId(starting), PlayerId(observer))
    };
    let m = cfg.rollouts as usize;
    let mut out = String::new();
    for observer in 0..2u8 {
        let obs = observer as usize;
        let seeds: Vec<u64> = (0..m as u64).map(|r| mix(seed ^ mix(0x4D55 ^ (r << 8) ^ observer as u64))).collect();
        let mut keep = new_opening(observer)?;
        let kept_hand = hand_ids(&keep);
        keep.keep()?;
        let keep_session = keep.into_session()?;
        let keep_scores = opening_rollouts(&keep_session, roll, obs, &seeds)?;
        let mut probe = new_opening(observer)?;
        probe.mulligan()?;
        let mull_hand = hand_ids(&probe);
        let mut seen = Vec::new();
        let mut bottoms = Vec::new();
        let half = (m / 2).max(1);
        for (index, &card) in mull_hand.iter().enumerate() {
            if seen.contains(&card) {
                continue;
            }
            seen.push(card);
            let mut opening = new_opening(observer)?;
            opening.mulligan()?;
            opening.keep()?;
            opening.bottom(&[index as u32])?;
            let session = opening.into_session()?;
            let scores = opening_rollouts(&session, roll, obs, &seeds[..half])?;
            bottoms.push(json!({"card":card,"scores":scores}));
        }
        let row = json!({"kind":"mulligan","game":game,"seed":seed,"decks":[decks[0].id,decks[1].id],
            "starting_player":starting,"observer":observer,"kept_hand":kept_hand,"keep_scores":keep_scores,
            "mulligan_hand":mull_hand,"bottoms":bottoms});
        out.push_str(&row.to_string());
        out.push('\n');
    }
    let mut f = sink.lock().map_err(|_| "sink poisoned")?;
    f.write_all(out.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

/// Rollout-improved pilot: the pilot seat (playing `cfg.pilot_deck`) replaces
/// each multi-action choice by the best of the policy's top actions under
/// determinized policy rollouts; the opponent is the plain policy. Each game is
/// paired with the plain-policy game from the same seed.
fn run_pilot_game(
    cfg: &CensusConfigV1,
    game: u64,
    base: &mut FrozenPlayPolicyV1,
    roll: &mut FrozenPlayPolicyV1,
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
    let mut scores = [0.0f64; 2];
    let mut changed = 0u32;
    let mut evaluated_decisions = 0u32;
    for (variant_ix, improved) in [false, true].into_iter().enumerate() {
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
        let mut ordinal = 0u64;
        let score = loop {
            match session.current_response() {
                FastActorResponseV1::Terminal(t) => {
                    let natural = t.terminal_classification == TerminalClassificationV1::Natural;
                    break match t.winner {
                        Some(w) if natural => f64::from(u8::from(seat_index(w) == pilot)),
                        _ => 0.5,
                    };
                }
                FastActorResponseV1::Decision(d) => {
                    let sampled = base.select_fast_session_v1(&session)?;
                    let mut a = sampled;
                    if improved && seat_index(d.acting_player) == pilot && d.legal_action_count >= 2 {
                        ordinal += 1;
                        evaluated_decisions += 1;
                        roll.reset_sampling_v1([1, 2]);
                        let probs = softmax(&roll.score_fast_session_v1(&session)?.logits);
                        let mut order: Vec<usize> = (0..probs.len()).collect();
                        order.sort_by(|&x, &y| probs[y].partial_cmp(&probs[x]).unwrap());
                        let mut cand: Vec<usize> = order.into_iter().take(cfg.max_actions).collect();
                        if !cand.contains(&(sampled as usize)) {
                            cand.push(sampled as usize);
                        }
                        let root_seed = mix(seed ^ mix(0x9170 ^ ordinal));
                        let mut means = vec![0f64; cand.len()];
                        for r in 0..cfg.rollouts as u64 {
                            let det = mix(root_seed ^ (r + 1));
                            let pol = [mix(det ^ 0xA1), mix(det ^ 0xB2)];
                            for (i, &c) in cand.iter().enumerate() {
                                let mut s = match session.census_redeterminized_clone_v1(det) {
                                    Ok(s) => s,
                                    Err(_) => continue,
                                };
                                if s.step(d.episode_id, d.step, c as u32).is_err() {
                                    means[i] -= 1e3;
                                    continue;
                                }
                                roll.reset_sampling_v1(pol);
                                let (sc, _, _) = play_out(&mut s, roll, pilot)?;
                                means[i] += sc / f64::from(cfg.rollouts);
                            }
                        }
                        let own = cand.iter().position(|&c| c == sampled as usize).unwrap();
                        means[own] += 1.5 / f64::from(cfg.rollouts);
                        let best = (0..cand.len())
                            .max_by(|&x, &y| means[x].partial_cmp(&means[y]).unwrap())
                            .unwrap();
                        a = cand[best] as u32;
                        if a != sampled {
                            changed += 1;
                        }
                    }
                    session
                        .step(d.episode_id, d.step, a)
                        .map_err(|e| format!("{e:?}"))?;
                }
            }
        };
        scores[variant_ix] = score;
    }
    let row = json!({"kind":"pilot","game":game,"pilot_seat":pilot,
        "decks":[decks[0].id,decks[1].id],"pilot_deck":decks[pilot].id,"opp_deck":RUNTIME_DECKS[opp_deck].id,
        "starting_player":starting,"base":scores[0],"improved":scores[1],
        "evaluated_decisions":evaluated_decisions,"changed":changed});
    let mut f = sink.lock().map_err(|_| "sink poisoned")?;
    writeln!(f, "{row}").map_err(|e| e.to_string())?;
    Ok(())
}

fn named(v: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Object(m) => {
            if let Some(id) = m.get("card_db_id").and_then(Value::as_u64) {
                return json!(crate::rl::card_name(id as u16));
            }
            Value::Object(m.into_iter().filter(|(k, _)| k != "actor").map(|(k, x)| (k, named(x))).collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(named).collect()),
        x => x,
    }
}

fn zone_names(state: &crate::state::GameState, ids: &[crate::ids::ObjectId]) -> Vec<String> {
    ids.iter().map(|&o| state.objects.get(o).name.clone()).collect()
}

/// Plain-policy games with `cfg.pilot_deck` in the pilot seat. Logs every
/// pilot multi-action decision with the chosen and offered actions and a
/// compact view of the pilot's resources, for offline plan analysis.
fn run_trace_game(
    cfg: &CensusConfigV1,
    game: u64,
    base: &mut FrozenPlayPolicyV1,
    roll: &mut FrozenPlayPolicyV1,
    sink: &Mutex<std::fs::File>,
) -> Result<(), String> {
    let root_filter = std::env::var("ROOT_FILTER").ok();
    let mut roots = Vec::new();
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
    base.reset_sampling_v1(paired_policy_seeds_v1(seed));
    let mut rows = Vec::new();
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
                let a = base.select_fast_session_v1(&session)?;
                if seat_index(d.acting_player) == pilot && d.legal_action_count >= 2 {
                    let sem = session.diagnostic_current_action_semantics().ok_or("semantics")?;
                    if let Some(filter) = root_filter.as_deref() {
                        let hit = sem.iter().any(|x| {
                            named(serde_json::to_value(x).unwrap_or_default()).to_string().contains(filter)
                        });
                        if hit {
                            let mut root = evaluate_root(cfg, &session, roll, seed, rows.len() as u64)?;
                            root["game"] = json!(game);
                            root["base_chosen"] = json!(a);
                            root["named"] = json!(sem.iter().map(|x| named(serde_json::to_value(x).unwrap_or_default())).collect::<Vec<_>>());
                            roots.push(root);
                        }
                    }
                    let probs = softmax(&base.score_fast_session_v1(&session)?.logits);
                    let st = session.game_state();
                    let me = &st.players[pilot];
                    let op = &st.players[1 - pilot];
                    let cands: Vec<_> = sem.iter().map(|x| named(serde_json::to_value(x).unwrap_or_default())).collect();
                    rows.push(json!({"kind":"decision","game":game,"turn":st.turn,"step":format!("{:?}",st.step),
                        "active":st.active_player.0 as usize == pilot,"chosen":a,"probs":probs,"cands":cands,
                        "life":[me.life,op.life],"pool":me.mana_pool,"hand":zone_names(st,&me.hand),
                        "bf":zone_names(st,&me.battlefield),"gy":zone_names(st,&me.graveyard),"lib":me.library.len(),
                        "opp_bf":zone_names(st,&op.battlefield),"opp_hand":op.hand.len(),"stack":st.stack.len()}));
                }
                session
                    .step(d.episode_id, d.step, a)
                    .map_err(|e| format!("{e:?}"))?;
            }
        }
    };
    let st = session.game_state();
    let me = &st.players[pilot];
    let mut out = json!({"kind":"trace_game","game":game,"pilot_seat":pilot,"pilot_deck":decks[pilot].id,
        "opp_deck":RUNTIME_DECKS[opp_deck].id,"starting_player":starting,"score":score,"turns":turns,
        "final_life":[me.life,st.players[1-pilot].life],"final_lib":[me.library.len(),st.players[1-pilot].library.len()],
        "final_bf":zone_names(st,&me.battlefield),"final_gy":zone_names(st,&me.graveyard),"final_hand":zone_names(st,&me.hand)}).to_string();
    out.push('\n');
    for r in rows.into_iter().chain(roots) {
        out.push_str(&r.to_string());
        out.push('\n');
    }
    let mut f = sink.lock().map_err(|_| "sink poisoned")?;
    f.write_all(out.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

/// Scripted correction of the two Spy combo choices T1 gets wrong. `level`
/// 1: Dread Return targets Lotleth Giant when offered. Level 2 adds: Balustrade
/// Spy targets its controller when the controller's library holds no land, a
/// Dread Return is in library or graveyard, a Lotleth Giant is in library or
/// graveyard, and the controller has three creatures to sacrifice. The
/// controller knows its own library contents, so this uses no hidden info.
fn spy_combo_override(session: &FastActorSessionV1, seat: usize, level: u8) -> Option<u32> {
    use crate::card_def::{CardType, CARD_DEFS};
    let sem = session.diagnostic_current_action_semantics()?;
    let named_sem: Vec<serde_json::Value> =
        sem.iter().map(|x| named(serde_json::to_value(x).unwrap_or_default())).collect();
    let is = |v: &serde_json::Value, kind: &str, src: &str| v["action_kind"] == kind && v["source"] == src;
    if let Some(i) = named_sem.iter().position(|v| {
        is(v, "choose_target", "Dread Return") && v["target"]["object"] == "Lotleth Giant"
    }) {
        return Some(i as u32);
    }
    let spy_target = named_sem.iter().any(|v| is(v, "choose_target", "Balustrade Spy"));
    let spy_cast = named_sem.iter().position(|v| is(v, "cast_spell", "Balustrade Spy"));
    if level < 2 || !(spy_target || (level >= 3 && spy_cast.is_some())) {
        return None;
    }
    let st = session.game_state();
    let me = &st.players[seat];
    let names = |ids: &[crate::ids::ObjectId]| -> Vec<String> { zone_names(st, ids) };
    let lib_land = me.library.iter().any(|&o| CARD_DEFS[st.objects.get(o).card_def as usize].types.contains(&CardType::Land));
    let pool: Vec<String> = names(&me.library).into_iter().chain(names(&me.graveyard)).collect();
    let creatures = me
        .battlefield
        .iter()
        .filter(|&&o| CARD_DEFS[st.objects.get(o).card_def as usize].types.contains(&CardType::Creature))
        .count();
    let ready = !lib_land
        && pool.iter().any(|n| n == "Dread Return")
        && pool.iter().any(|n| n == "Lotleth Giant");
    if !spy_target {
        // Level 3: cast an offered Spy whenever it would complete the combo
        // (Spy itself is the third creature to sacrifice).
        return (ready && creatures >= 2).then(|| spy_cast.unwrap() as u32);
    }
    let live = ready && creatures >= 3;
    let want = if live { seat } else { 1 - seat };
    named_sem
        .iter()
        .position(|v| is(v, "choose_target", "Balustrade Spy") && v["target"]["player"] == format!("p{want}"))
        .map(|i| i as u32)
}

/// Paired plain vs scripted-fix games for the Spy pilot (pilot seat as in
/// `run_pilot_game`). Same seeds and sampling streams for both variants.
fn run_spyfix_game(
    cfg: &CensusConfigV1,
    game: u64,
    base: &mut FrozenPlayPolicyV1,
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
    let mut scores = [0.0f64; 4];
    let mut overrides = [0u32; 4];
    let mut turns = [0u32; 4];
    for level in 0..4u8 {
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
        scores[level as usize] = loop {
            match session.current_response() {
                FastActorResponseV1::Terminal(t) => {
                    let natural = t.terminal_classification == TerminalClassificationV1::Natural;
                    turns[level as usize] = session.game_state().turn;
                    break match t.winner {
                        Some(w) if natural => f64::from(u8::from(seat_index(w) == pilot)),
                        _ => 0.5,
                    };
                }
                FastActorResponseV1::Decision(d) => {
                    let mut a = base.select_fast_session_v1(&session)?;
                    if level > 0 && seat_index(d.acting_player) == pilot {
                        if let Some(o) = spy_combo_override(&session, pilot, level) {
                            if o != a {
                                overrides[level as usize] += 1;
                            }
                            a = o;
                        }
                    }
                    session
                        .step(d.episode_id, d.step, a)
                        .map_err(|e| format!("{e:?}"))?;
                }
            }
        };
    }
    let row = json!({"kind":"spyfix","game":game,"pilot_seat":pilot,"opp_deck":RUNTIME_DECKS[opp_deck].id,
        "starting_player":starting,"plain":scores[0],"dr_fix":scores[1],"full_fix":scores[2],"cast_fix":scores[3],
        "overrides":overrides,"turns":turns});
    let mut f = sink.lock().map_err(|_| "sink poisoned")?;
    writeln!(f, "{row}").map_err(|e| e.to_string())?;
    Ok(())
}

fn spy_ready(session: &FastActorSessionV1, seat: usize) -> (bool, usize) {
    use crate::card_def::{CardType, CARD_DEFS};
    let st = session.game_state();
    let me = &st.players[seat];
    let has = |o: crate::ids::ObjectId, t: CardType| CARD_DEFS[st.objects.get(o).card_def as usize].types.contains(&t);
    let pool: Vec<String> = zone_names(st, &me.library).into_iter().chain(zone_names(st, &me.graveyard)).collect();
    let ready = !me.library.iter().any(|&o| has(o, CardType::Land))
        && pool.iter().any(|n| n == "Dread Return")
        && pool.iter().any(|n| n == "Lotleth Giant");
    (ready, me.battlefield.iter().filter(|&&o| has(o, CardType::Creature)).count())
}

fn named_candidates(session: &FastActorSessionV1) -> Vec<serde_json::Value> {
    session
        .diagnostic_current_action_semantics()
        .unwrap_or_default()
        .iter()
        .map(|x| named(serde_json::to_value(x).unwrap_or_default()))
        .collect()
}

fn mass(probs: &[f64], cands: &[serde_json::Value], pred: impl Fn(&serde_json::Value) -> bool) -> f64 {
    cands.iter().zip(probs).filter(|(c, _)| pred(c)).map(|(_, p)| p).sum()
}

/// Spy combo learning probe. Positions are reached by the reference policy
/// (REF_SOURCE, default the candidate) playing both seats, so they do not
/// depend on the checkpoint under test. At the first ready Spy cast offer and
/// the first live Spy target choice, the candidate is scored on: probability
/// of casting Spy, probability of targeting itself, probability of choosing
/// Lotleth Giant at Dread Return after a forced self-mill (candidate pilots,
/// reference opponent, `rollouts` determinizations), and win rate from the
/// live target position under its own play.
fn run_spyprobe_game(
    cfg: &CensusConfigV1,
    game: u64,
    cand: &mut FrozenPlayPolicyV1,
    refp: &mut FrozenPlayPolicyV1,
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
    refp.reset_sampling_v1(paired_policy_seeds_v1(seed));
    let mut rows = Vec::new();
    let (mut cast_done, mut target_done) = (false, false);
    let is = |v: &serde_json::Value, kind: &str, src: &str| v["action_kind"] == kind && v["source"] == src;
    loop {
        let FastActorResponseV1::Decision(d) = session.current_response() else { break };
        if seat_index(d.acting_player) == pilot && d.legal_action_count >= 2 && !(cast_done && target_done) {
            let cands = named_candidates(&session);
            let (ready, creatures) = spy_ready(&session, pilot);
            let turn = session.game_state().turn;
            if !cast_done && ready && creatures >= 2 && cands.iter().any(|c| is(c, "cast_spell", "Balustrade Spy")) {
                cast_done = true;
                cand.reset_sampling_v1([1, 2]);
                let probs = softmax(&cand.score_fast_session_v1(&session)?.logits);
                let p = mass(&probs, &cands, |c| is(c, "cast_spell", "Balustrade Spy"));
                rows.push(json!({"kind":"probe_cast","game":game,"turn":turn,"opp_deck":RUNTIME_DECKS[opp_deck].id,"p_cast_spy":p}));
            }
            let target_ix = cands.iter().position(|c| {
                is(c, "choose_target", "Balustrade Spy") && c["target"]["player"] == format!("p{pilot}")
            });
            if !target_done && ready && creatures >= 3 && target_ix.is_some() {
                target_done = true;
                cand.reset_sampling_v1([1, 2]);
                let probs = softmax(&cand.score_fast_session_v1(&session)?.logits);
                let p_self = probs[target_ix.unwrap()];
                let m = cfg.rollouts as usize;
                let (mut lotleth, mut dr_reached, mut wins) = (0.0, 0u32, 0.0);
                for r in 0..m {
                    let det = mix(seed ^ mix(0x5059 ^ r as u64));
                    // Dread Return target after a forced self-mill.
                    let mut s = session.census_redeterminized_clone_v1(det)?;
                    s.step(d.episode_id, d.step, target_ix.unwrap() as u32).map_err(|e| format!("{e:?}"))?;
                    cand.reset_sampling_v1([mix(det ^ 0xA1), mix(det ^ 0xB2)]);
                    refp.reset_sampling_v1([mix(det ^ 0xC3), mix(det ^ 0xD4)]);
                    while let FastActorResponseV1::Decision(e) = s.current_response() {
                        if s.game_state().turn != turn {
                            break;
                        }
                        let a = if seat_index(e.acting_player) == pilot {
                            let cs = named_candidates(&s);
                            if cs.iter().any(|c| is(c, "choose_target", "Dread Return")) {
                                let pr = softmax(&cand.score_fast_session_v1(&s)?.logits);
                                lotleth += mass(&pr, &cs, |c| {
                                    is(c, "choose_target", "Dread Return") && c["target"]["object"] == "Lotleth Giant"
                                });
                                dr_reached += 1;
                                break;
                            }
                            cand.select_fast_session_v1(&s)?
                        } else {
                            refp.select_fast_session_v1(&s)?
                        };
                        s.step(e.episode_id, e.step, a).map_err(|e| format!("{e:?}"))?;
                    }
                    // Win rate from the live position under the candidate's own play.
                    let mut s = session.census_redeterminized_clone_v1(det)?;
                    cand.reset_sampling_v1([mix(det ^ 0xA1), mix(det ^ 0xB2)]);
                    refp.reset_sampling_v1([mix(det ^ 0xC3), mix(det ^ 0xD4)]);
                    let score = loop {
                        match s.current_response() {
                            FastActorResponseV1::Terminal(t) => {
                                let natural = t.terminal_classification == TerminalClassificationV1::Natural;
                                break match t.winner {
                                    Some(w) if natural => f64::from(u8::from(seat_index(w) == pilot)),
                                    _ => 0.5,
                                };
                            }
                            FastActorResponseV1::Decision(e) => {
                                let a = if seat_index(e.acting_player) == pilot {
                                    cand.select_fast_session_v1(&s)?
                                } else {
                                    refp.select_fast_session_v1(&s)?
                                };
                                s.step(e.episode_id, e.step, a).map_err(|e| format!("{e:?}"))?;
                            }
                        }
                    };
                    wins += score;
                }
                rows.push(json!({"kind":"probe_target","game":game,"turn":turn,"opp_deck":RUNTIME_DECKS[opp_deck].id,
                    "p_self":p_self,"dr_reached":dr_reached,"rollouts":m,
                    "p_lotleth":if dr_reached > 0 { lotleth / f64::from(dr_reached) } else { f64::NAN },
                    "win_from_position":wins / m as f64}));
                // The reference continuation must not depend on probe sampling.
                refp.reset_sampling_v1(paired_policy_seeds_v1(seed ^ mix(d.step)));
            }
        }
        let a = refp.select_fast_session_v1(&session)?;
        session.step(d.episode_id, d.step, a).map_err(|e| format!("{e:?}"))?;
    }
    let mut out = String::new();
    for r in rows {
        out.push_str(&r.to_string());
        out.push('\n');
    }
    let mut f = sink.lock().map_err(|_| "sink poisoned")?;
    f.write_all(out.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn run_v1(cfg: CensusConfigV1) -> Result<(), String> {
    let source: ExpandedModelSourceV1 = serde_json::from_slice(
        &std::fs::read(&cfg.source).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let (policy, identity) = load_expanded_inference_v1(&source)?;
    eprintln!("loaded {}", serde_json::to_string(&identity).unwrap_or_default());
    let reference = match std::env::var("REF_SOURCE") {
        Ok(path) => {
            let source: ExpandedModelSourceV1 =
                serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            let (p, id) = load_expanded_inference_v1(&source)?;
            eprintln!("reference {}", serde_json::to_string(&id).unwrap_or_default());
            p
        }
        Err(_) => policy.fork_for_collection_v3()?,
    };
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
            let mut refp = reference.fork_for_collection_v3()?;
            let (cfg, sink, next, done) = (&cfg, &sink, &next, &done);
            handles.push(scope.spawn(move || -> Result<(), String> {
                loop {
                    let g = next.fetch_add(1, Ordering::SeqCst);
                    if g >= end {
                        return Ok(());
                    }
                    let result = if cfg.mode == "mulligan" {
                        run_mulligan_game(cfg, g, &mut roll, sink)
                    } else if cfg.mode == "spyprobe" {
                        run_spyprobe_game(cfg, g, &mut base, &mut refp, sink)
                    } else if cfg.mode == "spyfix" {
                        run_spyfix_game(cfg, g, &mut base, sink)
                    } else if cfg.mode == "trace" {
                        run_trace_game(cfg, g, &mut base, &mut roll, sink)
                    } else if cfg.mode == "pilot" {
                        run_pilot_game(cfg, g, &mut base, &mut roll, sink)
                    } else {
                        run_game(cfg, g, &mut base, &mut roll, sink)
                    };
                    if let Err(e) = result {
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
