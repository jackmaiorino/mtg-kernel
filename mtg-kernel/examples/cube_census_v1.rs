//! Cost smoke for the cube drafting design note (collab LANES/lead/reviews,
//! DESIGN-CUBE-DRAFT-20261119.md). Measurement only: no training, no model
//! output, nothing written but one JSONL file of per-game receipts.
//!
//! Modes (all in the `eternal-fixtures` build):
//! - `jsonl40`: natural games between random 40-card decks of Full cube cards
//!   through the custom-deck session (`LimitedJsonlServerV1`, schema 5: engine
//!   priority windows, Foundations combat, London mulligans, public shroud),
//!   uniform over the legal menu.
//! - `fast60`: the same deck rule at 60 cards through the explicit-deck
//!   `FastActorSessionV1` (no observation, Pauper priority surface), uniform
//!   over the legal count. An engine-only proxy: that reset takes exactly 60.
//! - `actor60`: `EternalActorSessionV1` on the three mainboards its finite
//!   printed pool admits, scored by an untrained `current_tree_learner_v2`
//!   (H=32), sampled from its softmax. Times encode, input, forward, apply.
//! - `census`: which Full cube cards the tree learner's printed pool admits,
//!   and the reset refusal for a cube deck.
//!
//! The deck rule is a smoke input, declared before any data: each seat draws
//! two distinct colors; nonland cards are drawn without replacement from the
//! Full cube cards whose colors lie inside that pair (colorless included) and
//! that the other seat of the game does not hold; nonbasic lands are drawn
//! from all Full cube lands; the rest are basics of the two colors.
mtg_kernel::fast_global_allocator!();

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::current_tree_learner_v2::ModelV2;
use mtg_kernel::ids::PlayerId;
use mtg_kernel::limited_session_v1::LimitedJsonlServerV1;
use mtg_kernel::mana::ManaColor;
use mtg_kernel::rl_session::{EternalActorSessionV1, FastActorResponseV1, FastActorSessionV1};
use mtg_kernel::state::SplitMix64;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

const BASICS: [(&str, ManaColor); 5] = [
    ("Plains", ManaColor::W),
    ("Island", ManaColor::U),
    ("Swamp", ManaColor::B),
    ("Mountain", ManaColor::R),
    ("Forest", ManaColor::G),
];

struct Pool {
    /// (card id, cube name, color bits) of Full nonland cube cards.
    spells: Vec<(u16, String, u8)>,
    /// (card id, cube name) of Full nonbasic cube lands.
    lands: Vec<(u16, String)>,
    basics: [u16; 5],
    unresolved: Vec<String>,
    not_full: Vec<String>,
}

fn color_bit(color: ManaColor) -> u8 {
    match color {
        ManaColor::W => 1,
        ManaColor::U => 2,
        ManaColor::B => 4,
        ManaColor::R => 8,
        ManaColor::G => 16,
        ManaColor::C => 0,
    }
}

fn load_pool(cube: &str, gaps: &str) -> Pool {
    let cube: Value = serde_json::from_str(&std::fs::read_to_string(cube).unwrap()).unwrap();
    let gaps: Value = serde_json::from_str(&std::fs::read_to_string(gaps).unwrap()).unwrap();
    let mut gap_names: Vec<String> = Vec::new();
    for key in ["partial", "fdn_only", "missing"] {
        for name in gaps[key].as_array().unwrap() {
            gap_names.push(name.as_str().unwrap().to_owned());
        }
    }
    let mut pool = Pool {
        spells: Vec::new(),
        lands: Vec::new(),
        basics: BASICS.map(|(name, _)| card_id_by_name(name).expect("basic land")),
        unresolved: Vec::new(),
        not_full: Vec::new(),
    };
    for card in cube["cards"].as_array().unwrap() {
        let name = card["name"].as_str().unwrap();
        if gap_names.iter().any(|gap| gap == name) {
            continue;
        }
        let Some(id) = card_id_by_name(name) else {
            pool.unresolved.push(name.to_owned());
            continue;
        };
        let def = &CARD_DEFS[usize::from(id)];
        if def.is_token || !def.has_full_support() {
            pool.not_full.push(name.to_owned());
            continue;
        }
        if pool.basics.contains(&id) {
            continue;
        }
        if def.is_land {
            pool.lands.push((id, name.to_owned()));
        } else {
            let bits = def.colors.iter().fold(0, |bits, c| bits | color_bit(*c));
            pool.spells.push((id, name.to_owned(), bits));
        }
    }
    pool
}

fn draw(rng: &mut SplitMix64, bound: usize) -> usize {
    let bound = bound as u64;
    let threshold = bound.wrapping_neg() % bound;
    loop {
        let value = rng.next_u64();
        if value >= threshold {
            return (value % bound) as usize;
        }
    }
}

fn mix(parts: &[u64]) -> u64 {
    let mut rng = SplitMix64::seed(0x4355_4245_534d_4f4b);
    let mut out = rng.next_u64();
    for part in parts {
        let mut next = SplitMix64::seed(out ^ part.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        out = next.next_u64();
    }
    out
}

/// Two decks of `size` cards: `(ids, names with counts)` per seat.
fn deal_decks(
    pool: &Pool,
    seed: u64,
    game: u64,
    size: usize,
) -> [(Vec<u16>, Vec<(String, u32)>); 2] {
    let (spell_count, land_count) = if size == 40 { (23, 2) } else { (36, 4) };
    let mut rng = SplitMix64::seed(mix(&[seed, game, 1]));
    let mut taken_spells = vec![false; pool.spells.len()];
    let mut taken_lands = vec![false; pool.lands.len()];
    let mut decks = Vec::new();
    for _seat in 0..2 {
        let first = draw(&mut rng, 5);
        let mut second = draw(&mut rng, 4);
        if second >= first {
            second += 1;
        }
        let pair = color_bit(BASICS[first].1) | color_bit(BASICS[second].1);
        let mut ids = Vec::new();
        let mut rows: Vec<(String, u32)> = Vec::new();
        let mut candidates: Vec<usize> = (0..pool.spells.len())
            .filter(|&i| !taken_spells[i] && pool.spells[i].2 & !pair == 0)
            .collect();
        for _ in 0..spell_count.min(candidates.len()) {
            let at = draw(&mut rng, candidates.len());
            let index = candidates.swap_remove(at);
            taken_spells[index] = true;
            ids.push(pool.spells[index].0);
            rows.push((pool.spells[index].1.clone(), 1));
        }
        let mut land_candidates: Vec<usize> =
            (0..pool.lands.len()).filter(|&i| !taken_lands[i]).collect();
        for _ in 0..land_count.min(land_candidates.len()) {
            let at = draw(&mut rng, land_candidates.len());
            let index = land_candidates.swap_remove(at);
            taken_lands[index] = true;
            ids.push(pool.lands[index].0);
            rows.push((pool.lands[index].1.clone(), 1));
        }
        let basics = size - ids.len();
        let first_count = basics.div_ceil(2);
        for (color, count) in [(first, first_count), (second, basics - first_count)] {
            ids.extend(std::iter::repeat_n(pool.basics[color], count));
            rows.push((BASICS[color].0.to_owned(), count as u32));
        }
        decks.push((ids, rows));
    }
    let second = decks.pop().unwrap();
    [decks.pop().unwrap(), second]
}

fn deck_json(rows: &[(String, u32)]) -> Value {
    json!({"cards": rows.iter().map(|(name, count)| json!({"name": name, "count": count})).collect::<Vec<_>>()})
}

fn names(rows: &[(String, u32)]) -> Vec<String> {
    rows.iter()
        .map(|(name, count)| {
            if *count == 1 {
                name.clone()
            } else {
                format!("{count} {name}")
            }
        })
        .collect()
}

fn card_name(id: Option<u64>) -> Value {
    match id.and_then(|id| CARD_DEFS.get(id as usize)) {
        Some(def) => json!(def.name),
        None => Value::Null,
    }
}

fn brief(action: &Value) -> Value {
    let semantic = &action["semantic"];
    json!({
        "kind": semantic["action_kind"],
        "actor": semantic["actor"],
        "source": card_name(semantic["source"]["card_db_id"].as_u64()),
        "target": card_name(semantic["target"]["object"]["card_db_id"].as_u64()),
    })
}

struct Caps {
    max_decisions: u64,
    max_seconds: f64,
    /// Diagnostic only (`--trace PATH`, jsonl40): the decks, then every chosen
    /// action, flushed line by line, so a game that kills the process still
    /// leaves its last actions. Not used by any timed arm.
    trace: Option<String>,
}

fn play_jsonl40(pool: &Pool, seed: u64, game: u64, caps: &Caps) -> Value {
    let decks = deal_decks(pool, seed, game, 40);
    let env_seed = mix(&[seed, game, 2]);
    let mut rng = SplitMix64::seed(mix(&[seed, game, 3]));
    let mut server = LimitedJsonlServerV1::new_with_public_shroud_v1();
    let mut request = json!({"schema_version":5,"request_type":"reset","request_id":"reset",
        "decks":[deck_json(&decks[0].1), deck_json(&decks[1].1)],
        "episode_id":game,"env_seed":env_seed,
        "max_physical_decisions":65536u64,"max_policy_steps":131072u64})
    .to_string();
    let mut trace = caps.trace.as_ref().map(|path| {
        let mut file = std::fs::File::create_new(path).expect("fresh --trace file");
        writeln!(
            file,
            "{}",
            json!({"game": game, "decks": [names(&decks[0].1), names(&decks[1].1)]})
        )
        .unwrap();
        file
    });
    let started = Instant::now();
    let (mut server_seconds, mut client_seconds) = (0.0f64, 0.0f64);
    let (mut decisions, mut multi, mut legal_sum, mut legal_max, mut reply_bytes) =
        (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut by_seat = [0u64; 2];
    let mut kinds: BTreeMap<String, u64> = BTreeMap::new();
    let mut recent: Vec<Value> = Vec::new();
    let mut end = json!({"end": "driver_decision_cap"});
    loop {
        let t0 = Instant::now();
        let raw = server.handle_line(&request);
        server_seconds += t0.elapsed().as_secs_f64();
        let t1 = Instant::now();
        reply_bytes += raw.len() as u64;
        let reply: Value = serde_json::from_str(&raw).unwrap();
        match reply["response_type"].as_str() {
            Some("error") => {
                end = json!({"end": "error", "code": reply["error"]["code"],
                    "message": reply["error"]["message"]});
                break;
            }
            Some("terminal") => {
                let terminal = &reply["terminal"];
                end = json!({"end": "terminal",
                    "classification": terminal["terminal_classification"],
                    "code": terminal["terminal_code"],
                    "outcome": terminal["terminal_outcome"],
                    "winner": terminal["winner"],
                    "reason": terminal["terminal_reason"],
                    "policy_step_count": terminal["policy_step_count"],
                    "physical_decision_count": terminal["physical_decision_count"]});
                break;
            }
            _ => {}
        }
        let decision = &reply["decision"];
        let actions = decision["legal_actions"].as_array().unwrap();
        if decisions >= caps.max_decisions {
            break;
        }
        if started.elapsed().as_secs_f64() > caps.max_seconds {
            end = json!({"end": "driver_wall_cap"});
            break;
        }
        let count = actions.len() as u64;
        decisions += 1;
        multi += u64::from(count > 1);
        legal_sum += count;
        legal_max = legal_max.max(count);
        let action = &actions[draw(&mut rng, actions.len())];
        if action["semantic"]["actor"] == "p1" {
            by_seat[1] += 1;
        } else {
            by_seat[0] += 1;
        }
        *kinds
            .entry(
                action["semantic"]["action_kind"]
                    .as_str()
                    .unwrap_or("?")
                    .to_owned(),
            )
            .or_default() += 1;
        if let Some(file) = trace.as_mut() {
            writeln!(
                file,
                "{}",
                json!({"n": decisions, "legal": count, "action": brief(action),
                "semantic": action["semantic"]})
            )
            .unwrap();
            file.flush().unwrap();
        }
        recent.push(brief(action));
        if recent.len() > 10 {
            recent.remove(0);
        }
        request = json!({"schema_version":5,"request_type":"step",
            "request_id":format!("step-{decisions}"),
            "episode_id":decision["episode_id"],"expected_step":decision["step"],
            "selected_index":action["selected_index"],"selected_action_id":action["stable_id"]})
        .to_string();
        client_seconds += t1.elapsed().as_secs_f64();
    }
    let natural = end["code"] == "natural_game_over";
    let mut row = json!({"mode": "jsonl40", "game": game, "end": end,
        "decisions": decisions, "multi_action_decisions": multi,
        "decisions_by_seat": by_seat,
        "legal_sum": legal_sum, "legal_max": legal_max, "reply_bytes": reply_bytes,
        "seconds": started.elapsed().as_secs_f64(),
        "server_seconds": server_seconds, "client_seconds": client_seconds,
        "action_kinds": kinds});
    if !natural {
        row["last_actions"] = json!(recent);
        row["decks"] = json!([names(&decks[0].1), names(&decks[1].1)]);
    }
    row
}

fn play_fast60(pool: &Pool, seed: u64, game: u64, caps: &Caps) -> Value {
    let decks = deal_decks(pool, seed, game, 60);
    let env_seed = mix(&[seed, game, 2]);
    let mut rng = SplitMix64::seed(mix(&[seed, game, 3]));
    let started = Instant::now();
    let reset = FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2_with_starting_player_v1(
        game, env_seed, 65536, 131072,
        ["cube-smoke-0".to_owned(), "cube-smoke-1".to_owned()],
        [decks[0].0.clone(), decks[1].0.clone()],
        if game.is_multiple_of(2) { PlayerId::P0 } else { PlayerId::P1 },
    );
    let mut session = match reset {
        Ok(session) => session,
        Err(error) => {
            return json!({"mode": "fast60", "game": game,
                "end": {"end": "reset_error", "message": error.message},
                "decks": [names(&decks[0].1), names(&decks[1].1)]});
        }
    };
    let (mut decisions, mut multi, mut legal_sum, mut legal_max) = (0u64, 0u64, 0u64, 0u64);
    let mut end = json!({"end": "driver_decision_cap"});
    let mut response = session.current_response();
    loop {
        let header = match response {
            FastActorResponseV1::Terminal(terminal) => {
                end = json!({"end": "terminal",
                    "classification": terminal.terminal_classification,
                    "code": terminal.terminal_code,
                    "outcome": terminal.terminal_outcome,
                    "reason": terminal.terminal_reason,
                    "policy_step_count": terminal.policy_step_count,
                    "physical_decision_count": terminal.physical_decision_count});
                break;
            }
            FastActorResponseV1::Decision(header) => header,
        };
        if decisions >= caps.max_decisions {
            break;
        }
        if started.elapsed().as_secs_f64() > caps.max_seconds {
            end = json!({"end": "driver_wall_cap"});
            break;
        }
        let count = u64::from(header.legal_action_count);
        decisions += 1;
        multi += u64::from(count > 1);
        legal_sum += count;
        legal_max = legal_max.max(count);
        let index = draw(&mut rng, count as usize) as u32;
        response = match session.step(header.episode_id, header.step, index) {
            Ok(response) => response,
            Err(error) => {
                end = json!({"end": "step_error", "message": error.message});
                break;
            }
        };
    }
    let natural = end["code"] == "natural_game_over";
    let mut row = json!({"mode": "fast60", "game": game, "end": end,
        "decisions": decisions, "multi_action_decisions": multi,
        "legal_sum": legal_sum, "legal_max": legal_max,
        "seconds": started.elapsed().as_secs_f64()});
    if !natural {
        row["decks"] = json!([names(&decks[0].1), names(&decks[1].1)]);
    }
    row
}

fn snapshot_mainboard(snapshot: &Value, archetype: &str) -> Vec<u16> {
    let deck = snapshot["formats"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|format| format.as_array().unwrap())
        .find(|deck| deck["archetype"] == archetype)
        .unwrap();
    deck["mainboard"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| {
            let id = card_id_by_name(row[1].as_str().unwrap()).unwrap();
            std::iter::repeat_n(id, row[0].as_u64().unwrap() as usize)
        })
        .collect()
}

const ACTOR_DECKS: [&str; 3] = ["legacy-reanimator", "legacy-doomsday", "vintage-red-prison"];

fn play_actor60(snapshot: &Value, model: &ModelV2, seed: u64, game: u64, caps: &Caps) -> Value {
    let mut rng = SplitMix64::seed(mix(&[seed, game, 3]));
    let first = ACTOR_DECKS[(game % 3) as usize];
    let second = ACTOR_DECKS[((game / 3) % 3) as usize];
    let started = Instant::now();
    let reset = EternalActorSessionV1::reset_explicit_mainboards(
        game,
        mix(&[seed, game, 2]),
        (65536, 131072),
        [first.to_owned(), second.to_owned()],
        [
            snapshot_mainboard(snapshot, first),
            snapshot_mainboard(snapshot, second),
        ],
        if (game / 9).is_multiple_of(2) {
            PlayerId::P0
        } else {
            PlayerId::P1
        },
    );
    let mut session = match reset {
        Ok(session) => session,
        Err(error) => {
            return json!({"mode": "actor60", "game": game, "decks": [first, second],
                "end": {"end": "reset_error", "message": error.to_string()}});
        }
    };
    let (mut decisions, mut multi, mut legal_sum, mut legal_max) = (0u64, 0u64, 0u64, 0u64);
    let (mut encode_s, mut input_s, mut forward_s, mut apply_s) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut end = json!({"end": "driver_decision_cap"});
    let mut response = session.current_response();
    loop {
        let header = match response {
            Err(error) => {
                end = json!({"end": "refused", "stage": "response", "message": error.to_string()});
                break;
            }
            Ok(FastActorResponseV1::Terminal(terminal)) => {
                end = json!({"end": "terminal",
                    "classification": terminal.terminal_classification,
                    "code": terminal.terminal_code,
                    "outcome": terminal.terminal_outcome,
                    "reason": terminal.terminal_reason,
                    "policy_step_count": terminal.policy_step_count,
                    "physical_decision_count": terminal.physical_decision_count});
                break;
            }
            Ok(FastActorResponseV1::Decision(header)) => header,
        };
        if decisions >= caps.max_decisions {
            break;
        }
        if started.elapsed().as_secs_f64() > caps.max_seconds {
            end = json!({"end": "driver_wall_cap"});
            break;
        }
        let t0 = Instant::now();
        let decision = match session.encode(header) {
            Ok(decision) => decision,
            Err(error) => {
                end = json!({"end": "refused", "stage": "encode", "message": error.to_string()});
                break;
            }
        };
        encode_s += t0.elapsed().as_secs_f64();
        let t1 = Instant::now();
        let input = match session.training_input_v1(&decision) {
            Ok(input) => input,
            Err(error) => {
                end = json!({"end": "refused", "stage": "input", "message": error.to_string()});
                break;
            }
        };
        input_s += t1.elapsed().as_secs_f64();
        let t2 = Instant::now();
        let scored = match model.forward(&input) {
            Ok(scored) => scored,
            Err(error) => {
                end = json!({"end": "refused", "stage": "forward", "message": error.to_string()});
                break;
            }
        };
        forward_s += t2.elapsed().as_secs_f64();
        let count = scored.logits.len() as u64;
        decisions += 1;
        multi += u64::from(count > 1);
        legal_sum += count;
        legal_max = legal_max.max(count);
        // Sample from the softmax of the untrained logits.
        let top = scored
            .logits
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        let weights: Vec<f64> = scored.logits.iter().map(|l| (l - top).exp()).collect();
        let total: f64 = weights.iter().sum();
        let mut point = (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64 * total;
        let mut index = weights.len() - 1;
        for (at, weight) in weights.iter().enumerate() {
            if point < *weight {
                index = at;
                break;
            }
            point -= weight;
        }
        let t3 = Instant::now();
        response = match session.apply(&decision, index as u32) {
            Ok(response) => Ok(response),
            Err(error) => {
                end = json!({"end": "refused", "stage": "apply", "message": error.to_string()});
                break;
            }
        };
        apply_s += t3.elapsed().as_secs_f64();
    }
    json!({"mode": "actor60", "game": game, "decks": [first, second], "end": end,
        "decisions": decisions, "multi_action_decisions": multi,
        "legal_sum": legal_sum, "legal_max": legal_max,
        "seconds": started.elapsed().as_secs_f64(),
        "encode_seconds": encode_s, "input_seconds": input_s,
        "forward_seconds": forward_s, "apply_seconds": apply_s})
}

fn census(pool: &Pool) -> Value {
    let mut admitted = Vec::new();
    let mut refused = 0u64;
    for (id, name) in pool
        .spells
        .iter()
        .map(|(id, name, _)| (*id, name))
        .chain(pool.lands.iter().map(|(id, name)| (*id, name)))
    {
        if mtg_kernel::eternal_actor_printed_v1::printing_v1(id).is_ok() {
            admitted.push(name.clone());
        } else {
            refused += 1;
        }
    }
    let basics_admitted: Vec<&str> = BASICS
        .iter()
        .zip(pool.basics)
        .filter(|(_, id)| mtg_kernel::eternal_actor_printed_v1::printing_v1(*id).is_ok())
        .map(|((name, _), _)| *name)
        .collect();
    let decks = deal_decks(pool, 1, 0, 60);
    let refusal = match EternalActorSessionV1::reset_explicit_mainboards(
        0,
        1,
        (65536, 131072),
        ["cube-0".to_owned(), "cube-1".to_owned()],
        [decks[0].0.clone(), decks[1].0.clone()],
        PlayerId::P0,
    ) {
        Ok(_) => "accepted".to_owned(),
        Err(error) => error.to_string(),
    };
    let forty = deal_decks(pool, 1, 0, 40);
    let refusal40 = match EternalActorSessionV1::reset_explicit_mainboards(
        0,
        1,
        (65536, 131072),
        ["cube-0".to_owned(), "cube-1".to_owned()],
        [forty[0].0.clone(), forty[1].0.clone()],
        PlayerId::P0,
    ) {
        Ok(_) => "accepted".to_owned(),
        Err(error) => error.to_string(),
    };
    json!({"mode": "census",
        "card_db_hash": format!("{:016x}", mtg_kernel::card_def::KERNEL_CARDDB_HASH),
        "registry_cards": CARD_DEFS.len(),
        "full_cube_nonland": pool.spells.len(), "full_cube_nonbasic_lands": pool.lands.len(),
        "unresolved_names": pool.unresolved, "listed_full_but_not_full": pool.not_full,
        "tree_learner_printed_pool_admits": admitted, "tree_learner_printed_pool_refuses": refused,
        "basics_admitted": basics_admitted,
        "eternal_actor_reset_cube60": refusal, "eternal_actor_reset_cube40": refusal40})
}

/// One row per Full, nontoken registry card: what the engine holds about the
/// card in typed form, as Debug text of the effect trees its definition
/// returns. Input to the structure census of the design note; no game runs.
fn structure(pool: &Pool) -> Vec<Value> {
    let cube: Vec<u16> = pool
        .spells
        .iter()
        .map(|(id, _, _)| *id)
        .chain(pool.lands.iter().map(|(id, _)| *id))
        .chain(pool.basics)
        .collect();
    let mut rows = Vec::new();
    for (index, def) in CARD_DEFS.iter().enumerate() {
        if def.is_token || !def.has_full_support() {
            continue;
        }
        let id = index as u16;
        let mut modes = Vec::new();
        for mode in def
            .mode2
            .iter()
            .chain(def.mode3.iter())
            .chain(def.additional_modes)
        {
            modes.push(json!({"target": format!("{:?}", mode.target_spec),
                "effect": format!("{:?}", (mode.effect)())}));
        }
        let activated: Vec<Value> = def
            .activated_abilities
            .iter()
            .map(|ability| {
                json!({"cost": format!("{:?}", ability.cost),
                    "target": format!("{:?}", ability.target_spec),
                    "zone": format!("{:?}", ability.activation_zone),
                    "sorcery_speed": ability.sorcery_speed_only,
                    "effect": format!("{:?}", (ability.effect)())})
            })
            .collect();
        let triggers: Vec<Value> = mtg_kernel::trigger::triggers_for(id)
            .iter()
            .map(|trigger| {
                json!({"condition": format!("{:?}", trigger.condition),
                    "zone": format!("{:?}", trigger.home_zone),
                    "effect": format!("{:?}", (trigger.effect)())})
            })
            .collect();
        let mut present = Vec::new();
        for (name, on) in [
            ("alt_cost", def.alt_cost.is_some()),
            ("kicker", def.kicker_cost.is_some()),
            ("additional_cost", def.additional_cost.is_some()),
            ("flashback", def.flashback.is_some()),
            ("escape", def.escape.is_some()),
            ("madness", def.madness_cost.is_some()),
            ("plot", def.plot_cost.is_some()),
            ("adventure", def.adventure.is_some()),
            ("omen", def.omen.is_some()),
            ("transform", def.transform_face.is_some()),
            ("saga", def.saga.is_some()),
            ("equipment", def.equipment.is_some()),
            ("attachment", def.attachment.is_some()),
            ("bestow", def.bestow.is_some()),
            ("delve", def.delve),
            ("changeling", def.changeling),
            ("ward", def.ward_cost.is_some()),
            ("uncounterable", def.spell_cannot_be_countered),
            ("animation", def.animation.is_some()),
            ("enters_tapped", def.enters_battlefield_tapped),
            ("loyalty", def.starting_loyalty.is_some()),
            (
                "mana_ability",
                (def.mana_ability)().is_some() || def.mana_ability_def.is_some(),
            ),
        ] {
            if on {
                present.push(name);
            }
        }
        rows.push(json!({
            "id": id, "name": def.name, "cube": cube.contains(&id),
            "types": format!("{:?}", def.types), "subtypes": format!("{:?}", def.subtypes),
            "colors": format!("{:?}", def.colors), "mana_value": def.mana_value,
            "cost": format!("{:?}", def.cost),
            "power": def.power, "toughness": def.toughness,
            "keywords": def.keywords.0, "is_land": def.is_land,
            "target": format!("{:?}", def.target_spec),
            "spell": (def.spell_effect)().map(|op| format!("{op:?}")),
            "modes": modes, "activated": activated, "triggers": triggers,
            "present": present,
        }));
    }
    rows
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|at| args.get(at + 1))
        .cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = arg(&args, "--mode").expect("--mode");
    let cube = arg(&args, "--cube").expect("--cube");
    let gaps = arg(&args, "--gaps").expect("--gaps");
    let games: u64 = arg(&args, "--games").map_or(0, |v| v.parse().unwrap());
    let first: u64 = arg(&args, "--first-game").map_or(0, |v| v.parse().unwrap());
    let seed: u64 = arg(&args, "--seed").map_or(20261119, |v| v.parse().unwrap());
    let workers: usize = arg(&args, "--workers").map_or(1, |v| v.parse().unwrap());
    let caps = Caps {
        max_decisions: arg(&args, "--max-decisions").map_or(8000, |v| v.parse().unwrap()),
        max_seconds: arg(&args, "--max-seconds").map_or(300.0, |v| v.parse().unwrap()),
        trace: arg(&args, "--trace"),
    };
    assert!(
        caps.trace.is_none() || (games <= 1 && workers == 1),
        "--trace records one game on one worker"
    );
    assert!(
        matches!(
            mode.as_str(),
            "jsonl40" | "fast60" | "actor60" | "census" | "structure"
        ),
        "unknown --mode {mode}"
    );
    assert!(
        mode != "actor60" || arg(&args, "--snapshot").is_some(),
        "--snapshot is required for actor60"
    );
    let pool = load_pool(&cube, &gaps);
    if mode == "census" {
        println!("{}", census(&pool));
        return;
    }
    if mode == "structure" {
        let path = arg(&args, "--out").expect("--out");
        let mut file = std::fs::File::create_new(&path).expect("fresh --out file");
        let rows = structure(&pool);
        for row in &rows {
            writeln!(file, "{row}").unwrap();
        }
        println!(
            "{}",
            json!({"mode": "structure", "rows": rows.len(), "out": path})
        );
        return;
    }
    let snapshot: Option<Value> = arg(&args, "--snapshot")
        .map(|path| serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap());
    let model = ModelV2::deterministic_initialization();
    let out_path = arg(&args, "--out").expect("--out");
    let out = Mutex::new(std::fs::File::create_new(&out_path).expect("fresh --out file"));
    let next = AtomicU64::new(first);
    let last = first + games;
    // Panics are receipts too: a game that panics is written as one.
    std::panic::set_hook(Box::new(|_| {}));
    let started = Instant::now();
    std::thread::scope(|scope| {
        for worker in 0..workers {
            let (pool, caps, out, next, mode, snapshot, model) =
                (&pool, &caps, &out, &next, &mode, &snapshot, &model);
            std::thread::Builder::new()
                .name(format!("cube-smoke-{worker}"))
                .stack_size(256 * 1024 * 1024)
                .spawn_scoped(scope, move || loop {
                    let game = next.fetch_add(1, Ordering::SeqCst);
                    if game >= last {
                        break;
                    }
                    let t0 = Instant::now();
                    let result =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            match mode.as_str() {
                                "jsonl40" => play_jsonl40(pool, seed, game, caps),
                                "fast60" => play_fast60(pool, seed, game, caps),
                                "actor60" => play_actor60(
                                    snapshot.as_ref().expect("--snapshot"),
                                    model,
                                    seed,
                                    game,
                                    caps,
                                ),
                                other => panic!("unknown mode {other}"),
                            }
                        }));
                    let mut row = match result {
                        Ok(row) => row,
                        Err(payload) => {
                            let message = payload
                                .downcast_ref::<String>()
                                .cloned()
                                .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                                .unwrap_or_else(|| "panic".to_owned());
                            let size = if mode == "jsonl40" { 40 } else { 60 };
                            let decks = deal_decks(pool, seed, game, size);
                            json!({"mode": mode, "game": game,
                                "end": {"end": "panic", "message": message},
                                "seconds": t0.elapsed().as_secs_f64(),
                                "decks": [names(&decks[0].1), names(&decks[1].1)]})
                        }
                    };
                    row["worker"] = json!(worker);
                    // One write per receipt, so a worker that dies mid-run never leaves half a line.
                    let line = format!("{row}\n");
                    out.lock().unwrap().write_all(line.as_bytes()).unwrap();
                })
                .unwrap();
        }
    });
    println!(
        "{}",
        json!({"mode": mode, "games": games, "first_game": first, "seed": seed,
            "workers": workers, "wall_seconds": started.elapsed().as_secs_f64(),
            "max_decisions": caps.max_decisions, "max_seconds": caps.max_seconds,
            "card_db_hash": format!("{:016x}", mtg_kernel::card_def::KERNEL_CARDDB_HASH),
            "out": out_path})
    );
}
