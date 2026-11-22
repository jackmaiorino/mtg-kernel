//! The planned line through a declaration (`private_line`, human play): the
//! engine asks an attack one creature at a time (`choose_attacker_inclusion`)
//! and a block one attacker and blocker pair at a time
//! (`choose_blocker_inclusion`). The host's "bot's line" at a declaration
//! answers those questions in order along the model's own answers, so the
//! line must play every substep of a declaration of the line's seat on the
//! clone.
//!
//! Proven here, through the bridge's own protocol lines, from the first
//! question of every attack and block declaration of the person's seat in
//! live games:
//!
//! 1. Inside a declaration the line never stops: each substep's answer is the
//!    seat's next question of the same declaration.
//! 2. Each line view is the decision the bridge itself poses after the same
//!    answers (a copy of the game stepped with them): the same candidates and
//!    model input, so a model read of a line view is the read the seat would
//!    get there.
//! 3. The live game is untouched: byte-equal, line for line, to the same game
//!    without any line, to its natural end.

use crate::agent_bridge_v1::{SpellbenchBridgeServerV1, SpellbenchEngineIdentityV1};
use crate::state::SplitMix64;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

static REQUESTS: AtomicU64 = AtomicU64::new(0);

const PERSON: &str = "p0";
const ATTACK: &str = "choose_attacker_inclusion";
const BLOCK: &str = "choose_blocker_inclusion";

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

fn step(
    server: &mut SpellbenchBridgeServerV1,
    game: &str,
    id: &str,
    response: &Value,
    row: usize,
) -> String {
    let candidate = &response["candidates"][row];
    server.handle_line(&request(
        "step",
        id,
        json!({"game_id": game, "expected_step": response["step"],
               "selection": {"candidate_id": candidate["candidate_id"],
                             "semantic_echo": candidate["semantic"]}}),
    ))
}

fn private_line(
    server: &mut SpellbenchBridgeServerV1,
    game: &str,
    root: &Value,
    ids: &[usize],
) -> Value {
    parse(&server.handle_line(&request(
        "private_line",
        &format!("decl-line-{}", REQUESTS.fetch_add(1, Ordering::Relaxed)),
        json!({"game_id": game, "expected_step": root["step"], "candidate_ids": ids}),
    )))
}

/// The declaration a decision of the person's seat is a question of: every
/// candidate an attack inclusion or every candidate a block inclusion.
fn declaration(response: &Value) -> Option<&'static str> {
    if response["response_type"] != "decision" || response["acting_seat"] != PERSON {
        return None;
    }
    let candidates = response["candidates"].as_array()?;
    [ATTACK, BLOCK].into_iter().find(|kind| {
        !candidates.is_empty()
            && candidates
                .iter()
                .all(|candidate| candidate["semantic"]["kind"] == *kind)
    })
}

/// The line's answer at the `k`-th question, from the view alone: include on
/// even questions and leave out on odd ones where the model's rows offer it,
/// else the first row. Reaches multi-blocker and multi-attacker declarations.
fn answer(view: &Value, k: usize) -> usize {
    let rows = rows(view);
    let want = k % 2 == 0;
    rows.iter()
        .copied()
        .find(|row| view["candidates"][*row]["semantic"]["include"] == want)
        .unwrap_or(rows[0])
}

/// What the seat is shown of a decision: its candidates, acting seat and model
/// input (less its step counter, which the planner never sends).
fn posed(view: &Value) -> Value {
    let mut model_input = view["extensions"]["x_kernel_flat_v7"].clone();
    model_input.as_object_mut().unwrap().remove("step");
    json!({"candidates": view["candidates"], "acting_seat": view["acting_seat"],
           "model_input": model_input})
}

#[derive(Default, Debug)]
struct Tally {
    attacks: u64,
    blocks: u64,
    substeps: u64,
    multi: u64,
}

/// The line from the first question of a declaration, through all of it,
/// against a copy of the game stepped with the same answers.
fn through(
    lined: &mut SpellbenchBridgeServerV1,
    game: &str,
    root: &Value,
    kind: &str,
    tally: &mut Tally,
) {
    let mut stepper = lined.clone_game_for_test();
    let (mut ids, mut view, mut k) = (Vec::new(), root.clone(), 0_usize);
    loop {
        let row = answer(&view, k);
        ids.push(row);
        let line = private_line(lined, game, root, &ids);
        let stepped = parse(&step(&mut stepper, game, &format!("ref-{k}"), &view, row));
        tally.substeps += 1;
        if declaration(&stepped) != Some(kind) {
            // The declaration is complete; what follows is the ordinary line.
            assert!(
                matches!(
                    line["response_type"].as_str(),
                    Some("private_line" | "private_line_stop")
                ),
                "{line}"
            );
            if k > 0 {
                tally.multi += 1;
            }
            return;
        }
        assert_eq!(
            line["response_type"], "private_line",
            "{game}: the line stopped inside a declaration at question {k}: {line}"
        );
        assert_eq!(
            posed(&line["view"]),
            posed(&stepped),
            "{game}: question {k}"
        );
        view = line["view"].clone();
        k += 1;
    }
}

fn play(seed: u64, person: &str, opponent: &str, tally: &mut Tally) {
    let game = format!("decl-{seed}-{person}-{opponent}");
    let (mut plain, mut lined) = (london_server(), london_server());
    let mut a = plain.handle_line(&reset_request(&game, person, opponent, seed));
    let mut b = lined.handle_line(&reset_request(&game, person, opponent, seed));
    let mut rng = SplitMix64::seed(seed);
    let mut previous: Option<&'static str> = None;
    for n in 0..20_000_u64 {
        assert_eq!(a, b, "{game}: line {n} differs");
        let response = parse(&a);
        if response["response_type"] != "decision" {
            assert_eq!(response["classification"], "natural", "{game}");
            return;
        }
        let kind = declaration(&response);
        // The first question of a declaration: the block root, or an attack
        // question right after a decision that was not one.
        let first = match kind {
            Some(BLOCK) => {
                response["extensions"]["x_kernel_v2_support"]["at_block_root"] == json!(true)
            }
            Some(_) => previous != kind,
            None => false,
        };
        if first {
            let kind = kind.unwrap();
            if kind == ATTACK {
                tally.attacks += 1;
            } else {
                tally.blocks += 1;
            }
            through(&mut lined, &game, &response, kind, tally);
        }
        previous = kind;
        let rows = rows(&response);
        let row = rows[(rng.next_u64() % rows.len() as u64) as usize];
        a = step(&mut plain, &game, &format!("s{n}"), &response, row);
        b = step(&mut lined, &game, &format!("s{n}"), &response, row);
    }
    panic!("{game}: the game did not end");
}

#[test]
fn a_planned_line_plays_every_question_of_a_declaration_on_the_clone() {
    let mut tally = Tally::default();
    for seed in 1..=3_u64 {
        for (person, opponent) in [
            ("Faeries", "Burn"),
            ("Burn", "Faeries"),
            ("Affinity", "Faeries"),
            ("Faeries", "Rally"),
        ] {
            play(seed, person, opponent, &mut tally);
        }
    }
    eprintln!("declarations through the line: {tally:?}");
    assert!(
        tally.attacks > 20 && tally.blocks > 10 && tally.multi > 10,
        "{tally:?}"
    );
}
