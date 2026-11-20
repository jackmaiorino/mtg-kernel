//! The advisor's planned line in human play (`private_line`, a request of the
//! human-play London bridge only): the person's own choices played forward on
//! a clone of the game, the opponent's bare priority passes taken for them,
//! stopping before anything hidden or random.
//!
//! Proven here, through the bridge's own protocol lines:
//!
//! 1. The request never changes the live game: a game that answers a planned
//!    line at every decision of the person's seat is byte-equal, line for
//!    line, to the same game without them, to its natural end.
//! 2. The opponent's hidden hand never shows: two games equal but for one
//!    card of the opponent's hidden hand (a Lightning Bolt they could answer
//!    with, against a card they could not) give equal planned lines (the
//!    person's candidates and model input at every step, the stop reason and
//!    result) from every decision of the person's seat.
//! 3. A line stops at an opponent decision that is not a priority pass, at a
//!    draw or other hidden change, at a random outcome, at the end of the turn
//!    and at a game end, with the result; other servers refuse the request.

use crate::agent_bridge_v1::{
    SpellbenchBridgeOptionsV1, SpellbenchBridgeServerV1, SpellbenchEngineIdentityV1,
};
use crate::state::SplitMix64;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Every request carries its own id (the server answers a repeated id from its cache).
static REQUESTS: AtomicU64 = AtomicU64::new(0);

const PERSON: &str = "p0";

fn request(kind: &str, id: &str, extra: Value) -> String {
    let mut value = json!({"protocol": "spellbench/v1", "request_type": kind, "request_id": id});
    for (key, item) in extra.as_object().unwrap() {
        value[key] = item.clone();
    }
    serde_json::to_string(&value).unwrap()
}

fn reset_request(game: &str, person: &str, opponent: &str, seed: u64) -> String {
    request(
        "reset",
        &format!("reset-{game}"),
        json!({"game_id": game, "format": "pauper-bo1", "game_seed": seed,
               "max_decisions": 100_000, "max_steps": 1_000_000,
               "engine_window_seats": [PERSON], "auto_graveyard_order": true,
               "seats": [{"seat": "p0", "deck": {"catalog_id": person}},
                         {"seat": "p1", "deck": {"catalog_id": opponent}}]}),
    )
}

fn london_server() -> SpellbenchBridgeServerV1 {
    SpellbenchBridgeServerV1::with_pauper_london_human_v1(
        SpellbenchEngineIdentityV1::with_source_revision(None),
    )
}

fn parse(line: &str) -> Value {
    serde_json::from_str(line).unwrap()
}

fn rows(response: &Value) -> Vec<usize> {
    response["extensions"]["x_kernel_flat_v7"]["row_candidate_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_u64().unwrap() as usize)
        .collect()
}

/// A seeded row of the model's own list (either seat, the live game).
fn seeded(response: &Value, rng: &mut SplitMix64) -> usize {
    let rows = rows(response);
    rows[(rng.next_u64() % rows.len() as u64) as usize]
}

/// The person's line choice, from the view alone (so equal views give equal
/// choices): the first row that is not a pass, else the first row. Plays
/// lands, casts spells and attacks wherever it can, which is what reaches
/// combat, the opponent's responses and game ends.
fn line_choice(view: &Value) -> usize {
    let candidates = view["candidates"].as_array().unwrap();
    let rows = rows(view);
    rows.iter()
        .copied()
        .find(|row| candidates[*row]["semantic"]["kind"] != "pass")
        .unwrap_or(rows[0])
}

fn step(server: &mut SpellbenchBridgeServerV1, game: &str, n: u64, response: &Value, row: usize) -> String {
    let candidate = &response["candidates"][row];
    server.handle_line(&request(
        "step",
        &format!("s{n}"),
        json!({"game_id": game, "expected_step": response["step"],
               "selection": {"candidate_id": candidate["candidate_id"],
                             "semantic_echo": candidate["semantic"]}}),
    ))
}

fn private_line(server: &mut SpellbenchBridgeServerV1, game: &str, root: &Value, ids: &[usize]) -> Value {
    parse(&server.handle_line(&request(
        "private_line",
        &format!("line-{}", REQUESTS.fetch_add(1, Ordering::Relaxed)),
        json!({"game_id": game, "expected_step": root["step"], "candidate_ids": ids}),
    )))
}

/// What of a line answer may reach the person's panel: the stop reason and
/// result, or the person's next decision's candidates and model input. The
/// counters of the view (`step`, the observation's own step and decision
/// numbers) are dropped by the planner and are left out here too.
fn shown(answer: &Value) -> Value {
    match answer["response_type"].as_str() {
        Some("private_line") => {
            let view = &answer["view"];
            // Never a view of a choice that shows library cards.
            let purpose = view["extensions"]["x_kernel_v2_support"]["choice"]["purpose"].as_str();
            assert!(
                !matches!(purpose, Some("search" | "mill" | "undercity_throne" | "look_select" | "reveal_partition"
                    | "look" | "scry" | "reveal" | "explore" | "surveil" | "owner_library_position" | "shuffle"
                    | "other")),
                "a line view shows library cards: {purpose:?}"
            );
            let mut model_input = view["extensions"]["x_kernel_flat_v7"].clone();
            // A counter, like the view's own: the planner's scorer requests
            // never carry it (their step is a constant).
            model_input.as_object_mut().unwrap().remove("step");
            json!({"candidates": view["candidates"],
                   "model_input": model_input,
                   "acting_seat": view["acting_seat"]})
        }
        Some("private_line_stop") => {
            json!({"stop": answer["reason"], "result": answer["result"]})
        }
        _ => panic!("unexpected line answer: {answer}"),
    }
}

/// The whole line from `root` (a decision of the person's seat), as the
/// planner plays it: at most `bound` choices, each read from the view the
/// previous one led to. Returns what may be shown at each step, and the stop.
fn whole_line(server: &mut SpellbenchBridgeServerV1, game: &str, root: &Value, bound: usize) -> Vec<Value> {
    line_from(server, game, root, line_choice(root), bound)
}

/// The line from `root` whose first choice is `first`.
fn line_from(server: &mut SpellbenchBridgeServerV1, game: &str, root: &Value, first: usize, bound: usize) -> Vec<Value> {
    let mut ids = vec![first];
    let mut out = Vec::new();
    loop {
        let answer = private_line(server, game, root, &ids);
        out.push(shown(&answer));
        if answer["response_type"] != "private_line" || ids.len() >= bound {
            return out;
        }
        ids.push(line_choice(&answer["view"]));
    }
}

#[derive(Default, Debug)]
struct Tally {
    lines: u64,
    stops: BTreeMap<String, u64>,
    results: BTreeMap<String, u64>,
}

impl Tally {
    fn add(&mut self, line: &[Value]) {
        self.lines += 1;
        let last = line.last().unwrap();
        let stop = last["stop"].as_str().unwrap_or("bound").to_string();
        *self.stops.entry(stop).or_default() += 1;
        if let Some(result) = last["result"].as_str() {
            *self.results.entry(result.to_string()).or_default() += 1;
        }
    }
}

/// One seed of property 1: the live game with a planned line at every
/// decision of the person's seat against the same game without, line for
/// line to the end.
fn untouched(seed: u64, tally: &mut Tally) {
    let game = format!("line-{seed}");
    let (mut plain, mut lined) = (london_server(), london_server());
    let mut a = plain.handle_line(&reset_request(&game, "Elves", "Burn", seed));
    let mut b = lined.handle_line(&reset_request(&game, "Elves", "Burn", seed));
    let mut rng = SplitMix64::seed(seed);
    for n in 0..20_000_u64 {
        assert_eq!(a, b, "seed {seed}: line {n} differs");
        let response = parse(&a);
        if response["response_type"] != "decision" {
            assert_eq!(response["classification"], "natural", "seed {seed}");
            return;
        }
        if response["acting_seat"] == PERSON {
            tally.add(&whole_line(&mut lined, &game, &response, 8));
        }
        let row = seeded(&response, &mut rng);
        a = step(&mut plain, &game, n, &response, row);
        b = step(&mut lined, &game, n, &response, row);
    }
    panic!("seed {seed}: the game did not end");
}

#[test]
fn a_planned_line_never_changes_the_live_game() {
    let mut tally = Tally::default();
    for seed in 1..=6_u64 {
        untouched(seed, &mut tally);
    }
    eprintln!("planned lines on live games: {tally:?}");
    assert!(tally.lines > 100, "{tally:?}");
}

/// The JSON paths at which two values differ (at most 20).
fn differing_paths(a: &Value, b: &Value, at: String) -> Vec<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            keys.into_iter()
                .flat_map(|k| differing_paths(&x.get(k).cloned().unwrap_or(Value::Null), &y.get(k).cloned().unwrap_or(Value::Null), format!("{at}/{k}")))
                .take(20)
                .collect()
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .enumerate()
            .flat_map(|(i, (p, q))| differing_paths(p, q, format!("{at}/{i}")))
            .take(20)
            .collect(),
        _ if a == b => Vec::new(),
        _ => vec![at],
    }
}

/// A decision while either seat is still in the pregame (the London phase is
/// not done, or a mulligan or a bottom is asked): the planner starts no line
/// in the person's pregame, and a hidden-card swap before the opponent has
/// kept would break the pregame's own bookkeeping of their hand.
fn pregame(response: &Value) -> bool {
    let phase = &response["extensions"]["x_kernel_v2_support"]["london"]["phase"];
    (phase.is_string() && phase != "done")
        || response["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|candidate| candidate["semantic"].to_string().contains("ulligan"))
}

/// One seed of property 2: at every decision of the person's seat, a copy of
/// the game whose opponent holds a Lightning Bolt where the other holds
/// another card (or the reverse), both unknown to the person, gives the same
/// line.
fn hand_blind(seed: u64, tally: &mut Tally, differing: &mut u64) {
    let game = format!("blind-{seed}");
    let mut live = london_server();
    let mut line = live.handle_line(&reset_request(&game, "Elves", "Burn", seed));
    let mut rng = SplitMix64::seed(seed ^ 0x5eed);
    for n in 0..20_000_u64 {
        let response = parse(&line);
        if response["response_type"] != "decision" {
            return;
        }
        if response["acting_seat"] == PERSON && !pregame(&response) {
            let mut same = live.clone_game_for_test();
            let mut other = live.clone_game_for_test();
            // In turn: the opponent's hand differs by a Bolt; the order of the
            // person's library differs; the order of the opponent's does.
            // No library swap while an effect choice is pending (a search binds
            // the library it shows; the person sees their library there).
            let effect_choice = response["extensions"]["x_kernel_v2_support"]["choice"].is_object();
            let differs = match n % 3 {
                0 => other.swap_opponent_bolt_for_test(1),
                1 if !effect_choice => other.swap_library_cards_for_test(0),
                2 if !effect_choice => other.swap_library_cards_for_test(1),
                _ => false,
            };
            if differs {
                *differing += 1;
                let first = whole_line(&mut same, &game, &response, 8);
                let second = whole_line(&mut other, &game, &response, 8);
                if first != second {
                    // Diagnosis: the altered copy's own first step, as the live game would play it.
                    let mut probe = live.clone_game_for_test();
                    match n % 3 {
                        0 => probe.swap_opponent_bolt_for_test(1),
                        1 => probe.swap_library_cards_for_test(0),
                        _ => probe.swap_library_cards_for_test(1),
                    };
                    let answer = step(&mut probe, &game, n, &response, line_choice(&response));
                    eprintln!("seed {seed}: decision {n}: altered copy, first step: {}", &answer[..answer.len().min(1500)]);
                }
                assert!(
                    first == second,
                    "seed {seed}: decision {n}: the line shows the opponent's hand at {:?}: {} against {}",
                    differing_paths(&json!(first), &json!(second), String::new()),
                    json!(first.iter().map(|v| v.get("stop").cloned().unwrap_or(json!("view"))).collect::<Vec<_>>()),
                    json!(second.iter().map(|v| v.get("stop").cloned().unwrap_or(json!("view"))).collect::<Vec<_>>())
                );
                tally.add(&first);
            }
        }
        let row = seeded(&response, &mut rng);
        line = step(&mut live, &game, n, &response, row);
    }
    panic!("seed {seed}: the game did not end");
}

#[test]
fn a_planned_line_is_blind_to_the_opponents_hidden_hand() {
    let (mut tally, mut differing) = (Tally::default(), 0_u64);
    for seed in 1..=6_u64 {
        hand_blind(seed, &mut tally, &mut differing);
    }
    eprintln!("hand-blind pairs: {differing}, {tally:?}");
    assert!(differing > 50, "{differing} {tally:?}");
    // Lines that reach the opponent's priority (anything past the first
    // choice) and lines that stop at their other decisions both occur.
    assert!(tally.stops.len() >= 3, "{tally:?}");
}

/// A line that casts Land Grant stops before its library search: the search
/// stages the library's cards without moving any, so only the choice's
/// purpose shows it (`LINE_HIDDEN_PURPOSES_V1`).
#[test]
fn a_planned_line_stops_before_a_library_search() {
    let (mut grants, mut stopped_hidden) = (0_u64, 0_u64);
    for seed in 1..=10_u64 {
        let game = format!("grant-{seed}");
        let mut live = london_server();
        let mut line = live.handle_line(&reset_request(&game, "Spy", "Burn", seed));
        let mut rng = SplitMix64::seed(seed);
        for n in 0..20_000_u64 {
            let response = parse(&line);
            if response["response_type"] != "decision" {
                break;
            }
            if response["acting_seat"] == PERSON && !pregame(&response) {
                let grant = rows(&response).into_iter().find(|row| {
                    let semantic = &response["candidates"][*row]["semantic"];
                    semantic["kind"] == "cast_spell" && semantic["source"]["card_name"] == "Land Grant"
                });
                if let Some(grant) = grant {
                    grants += 1;
                    let steps = line_from(&mut live, &game, &response, grant, 8);
                    stopped_hidden += u64::from(steps.last().unwrap()["stop"] == "unseen");
                }
            }
            let row = seeded(&response, &mut rng);
            line = step(&mut live, &game, n, &response, row);
        }
    }
    eprintln!("Land Grant lines: {grants}, stopped as hidden: {stopped_hidden}");
    assert!(grants > 0 && stopped_hidden > 0, "{grants} {stopped_hidden}");
}

#[test]
fn only_the_human_play_london_server_answers_a_planned_line() {
    let mut plain = SpellbenchBridgeServerV1::with_options(
        SpellbenchEngineIdentityV1::with_source_revision(None),
        SpellbenchBridgeOptionsV1 {
            x_kernel_flat_v4: true,
            x_kernel_infoset_v1: false,
            private_v2_support: true,
        },
    );
    let reset = parse(&plain.handle_line(&request(
        "reset",
        "reset-only",
        json!({"game_id": "only", "format": "pauper-bo1", "game_seed": 3,
               "max_decisions": 100_000, "max_steps": 1_000_000,
               "seats": [{"seat": "p0", "deck": {"catalog_id": "Burn"}},
                         {"seat": "p1", "deck": {"catalog_id": "Burn"}}]}),
    )));
    assert_eq!(reset["response_type"], "decision", "{reset}");
    let answer = private_line(&mut plain, "only", &reset, &[0]);
    assert_eq!(answer["response_type"], "error", "{answer}");
    let mut london = london_server();
    {
        let server = &mut london;
        let reset = parse(&server.handle_line(&reset_request("only", "Elves", "Burn", 3)));
        assert_eq!(reset["response_type"], "decision");
        let answer = private_line(server, "only", &reset, &[line_choice(&reset)]);
        assert!(
            matches!(answer["response_type"].as_str(), Some("private_line" | "private_line_stop")),
            "{answer}"
        );
        let stale = private_line(server, "only", &json!({"step": 99}), &[0]);
        assert_eq!(stale["response_type"], "error");
    }
}
