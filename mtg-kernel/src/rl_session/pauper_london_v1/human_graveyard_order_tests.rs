//! Graveyard order in human play (`auto_graveyard_order` on the human-play
//! bridge's reset): the training runtime rule `resolution-boundary-v1`, so a
//! batch of cards put into a graveyard at once is never posed to either seat
//! and keeps the order the effect bound it in, as in every training episode
//! of the play bots' lineages.
//!
//! Proven here, through the bridge's own protocol lines:
//!
//! 1. Spy mirrors with the rule on pose no graveyard ordering to either seat
//!    and end naturally; the same games with the rule off do pose them.
//! 2. Up to the first graveyard ordering the rule-off game poses, every line
//!    of the two games is byte-equal but for the `graveyard_order` marker of
//!    the private support block: both seats' decisions, candidates and
//!    model input.
//! 3. Absent and false keep every byte; only the human-play London server
//!    takes the field.
//! 4. The raw session refuses the rule in a game where a card reads
//!    graveyard order (Delve), as the training session does.

use super::*;
use crate::agent_bridge_v1::{
    SpellbenchBridgeOptionsV1, SpellbenchBridgeServerV1, SpellbenchEngineIdentityV1,
};
use crate::state::{SplitMix64, Zone};
use serde_json::{json, Value};

fn request(kind: &str, id: &str, extra: Value) -> String {
    let mut value = json!({"protocol": "spellbench/v1", "request_type": kind, "request_id": id});
    for (key, item) in extra.as_object().unwrap() {
        value[key] = item.clone();
    }
    serde_json::to_string(&value).unwrap()
}

fn reset_request(game: &str, deck: &str, seed: u64, auto: Option<bool>) -> String {
    let mut fields = json!({"game_id": game, "format": "pauper-bo1", "game_seed": seed,
        "max_decisions": 100_000, "max_steps": 1_000_000,
        "seats": [{"seat": "p0", "deck": {"catalog_id": deck}},
                  {"seat": "p1", "deck": {"catalog_id": deck}}]});
    if let Some(auto) = auto {
        fields["auto_graveyard_order"] = json!(auto);
    }
    request("reset", &format!("reset-{game}"), fields)
}

fn london_server() -> SpellbenchBridgeServerV1 {
    SpellbenchBridgeServerV1::with_pauper_london_human_v1(
        SpellbenchEngineIdentityV1::with_source_revision(None),
    )
}

fn parse(line: &str) -> Value {
    serde_json::from_str(line).unwrap()
}

/// A graveyard ordering decision: the support block's choice stage.
fn is_graveyard_order(response: &Value) -> bool {
    response["extensions"]["x_kernel_v2_support"]["choice"]["stage"] == "graveyard_order"
}

/// The line without the marker the rule-on game adds to its support block.
fn without_marker(line: &str) -> String {
    let mut value = parse(line);
    if let Some(support) = value
        .pointer_mut("/extensions/x_kernel_v2_support")
        .and_then(Value::as_object_mut)
    {
        support.remove("graveyard_order");
    }
    serde_json::to_string(&value).unwrap()
}

/// The row both games answer: Balustrade Spy whenever it can be cast, else a
/// seeded row of the model's own list.
fn pick(response: &Value, rng: &mut SplitMix64) -> usize {
    let candidates = response["candidates"].as_array().unwrap();
    let rows: Vec<usize> = response["extensions"]["x_kernel_flat_v7"]["row_candidate_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_u64().unwrap() as usize)
        .collect();
    let draw = rows[(rng.next_u64() % rows.len() as u64) as usize];
    rows.iter()
        .copied()
        .find(|row| {
            candidates[*row]["semantic"]["kind"] == "cast_spell"
                && candidates[*row]["semantic"]["source"]["card_name"] == "Balustrade Spy"
        })
        .unwrap_or(draw)
}

fn step(
    server: &mut SpellbenchBridgeServerV1,
    game: &str,
    n: u64,
    response: &Value,
    row: usize,
) -> String {
    let candidate = &response["candidates"][row];
    server.handle_line(&request(
        "step",
        &format!("s{n}"),
        json!({"game_id": game, "expected_step": response["step"],
               "selection": {"candidate_id": candidate["candidate_id"],
                             "semantic_echo": candidate["semantic"]}}),
    ))
}

#[derive(Default, Debug)]
struct Pair {
    shared_lines: u64,
    off_orderings: u64,
    on_orderings: u64,
    on_decisions: u64,
    on_natural: bool,
}

/// One seed: the rule-off and rule-on games side by side until the rule-off
/// game poses its first graveyard ordering (every line compared), then the
/// rule-on game alone to its end, and the rule-off game alone to its end.
fn spy_pair(seed: u64) -> Pair {
    let game = format!("gy-{seed}");
    let (mut off, mut on) = (london_server(), london_server());
    let mut off_line = off.handle_line(&reset_request(&game, "Spy", seed, None));
    let mut on_line = on.handle_line(&reset_request(&game, "Spy", seed, Some(true)));
    let (mut off_rng, mut on_rng) = (SplitMix64::seed(seed), SplitMix64::seed(seed));
    let mut out = Pair::default();
    let mut n = 0_u64;
    // Together, while nothing has been ordered.
    loop {
        let (a, b) = (parse(&off_line), parse(&on_line));
        if a["response_type"] == "decision" && is_graveyard_order(&a) {
            break;
        }
        assert_eq!(
            without_marker(&off_line),
            without_marker(&on_line),
            "seed {seed}: line {n} differs before any graveyard ordering"
        );
        if b["response_type"] == "decision" {
            assert_eq!(
                b["extensions"]["x_kernel_v2_support"]["graveyard_order"], "settled",
                "seed {seed}"
            );
        }
        if a["response_type"] != "decision" || n >= 6_000 {
            break;
        }
        out.shared_lines += 1;
        let (ra, rb) = (pick(&a, &mut off_rng), pick(&b, &mut on_rng));
        assert_eq!(ra, rb);
        off_line = step(&mut off, &game, n, &a, ra);
        on_line = step(&mut on, &game, n, &b, rb);
        n += 1;
    }
    // The rule-on game alone, to its end.
    let mut m = n;
    loop {
        let b = parse(&on_line);
        if b["response_type"] != "decision" {
            out.on_natural = b["classification"] == "natural";
            break;
        }
        assert!(m < 20_000, "seed {seed}: the rule-on game did not end");
        out.on_decisions += 1;
        out.on_orderings += u64::from(is_graveyard_order(&b));
        let row = pick(&b, &mut on_rng);
        on_line = step(&mut on, &game, m, &b, row);
        m += 1;
    }
    // The rule-off game alone, to its end (its orderings counted).
    let mut k = n;
    loop {
        let a = parse(&off_line);
        if a["response_type"] != "decision" || k >= 20_000 {
            break;
        }
        out.off_orderings += u64::from(is_graveyard_order(&a));
        let row = pick(&a, &mut off_rng);
        off_line = step(&mut off, &game, k, &a, row);
        k += 1;
    }
    out
}

#[test]
fn spy_mirrors_pose_no_graveyard_ordering_with_the_rule_and_equal_lines_before_the_first() {
    let mut total = Pair::default();
    let mut seeds_with_orderings = 0;
    for seed in 1..=12_u64 {
        let pair = spy_pair(seed);
        assert_eq!(pair.on_orderings, 0, "seed {seed}: {pair:?}");
        assert!(
            pair.on_natural,
            "seed {seed}: the rule-on game did not end naturally: {pair:?}"
        );
        seeds_with_orderings += u32::from(pair.off_orderings > 0);
        total.shared_lines += pair.shared_lines;
        total.off_orderings += pair.off_orderings;
        total.on_decisions += pair.on_decisions;
    }
    eprintln!(
        "graveyard order: 12 Spy mirrors, {seeds_with_orderings} with orderings when asked ({} ordering decisions), \
         {} lines equal before the first, {} decisions with the rule and none of them an ordering",
        total.off_orderings, total.shared_lines, total.on_decisions
    );
    assert!(
        seeds_with_orderings >= 3,
        "only {seeds_with_orderings} of 12 rule-off games posed a graveyard ordering"
    );
}

fn game_lines(
    server: &mut SpellbenchBridgeServerV1,
    deck: &str,
    seed: u64,
    auto: Option<bool>,
) -> Vec<String> {
    let game = format!("bytes-{seed}");
    let mut lines = vec![server.handle_line(&reset_request(&game, deck, seed, auto))];
    let mut rng = SplitMix64::seed(seed ^ 0xB17E);
    for n in 0..3_000 {
        let response = parse(lines.last().unwrap());
        if response["response_type"] != "decision" {
            break;
        }
        let row = pick(&response, &mut rng);
        lines.push(step(server, &game, n, &response, row));
    }
    lines
}

#[test]
fn absent_and_false_keep_every_byte_and_only_the_london_human_server_takes_the_field() {
    for (deck, seed) in [("Spy", 3_u64), ("Burn", 5)] {
        let absent = game_lines(&mut london_server(), deck, seed, None);
        let off = game_lines(&mut london_server(), deck, seed, Some(false));
        assert_eq!(absent, off, "{deck}");
        assert!(absent
            .iter()
            .all(|line| !line.contains("\"graveyard_order\":")));
    }
    for options in [
        SpellbenchBridgeOptionsV1::default(),
        SpellbenchBridgeOptionsV1 {
            x_kernel_flat_v4: true,
            x_kernel_infoset_v1: false,
            private_v2_support: true,
        },
    ] {
        let mut server = SpellbenchBridgeServerV1::with_options(
            SpellbenchEngineIdentityV1::with_source_revision(None),
            options,
        );
        let refused = parse(&server.handle_line(&reset_request("r1", "Spy", 3, Some(true))));
        assert_eq!(refused["error"]["code"], "malformed_request", "{refused}");
        let plain = parse(&server.handle_line(&reset_request("r2", "Spy", 3, Some(false))));
        assert_eq!(plain["response_type"], "decision", "{plain}");
    }
}

#[test]
fn the_raw_session_refuses_the_rule_where_a_card_reads_graveyard_order() {
    use crate::engine::RuntimeRulesV1;
    use crate::policy_observation_v6::tests::put;
    let decks: SessionDeckIdsV1 = ["Spy".to_string(), "Spy".to_string()];
    let fresh = || {
        RlEpisodeSessionV1::reset_pauper_london_human_v1(
            9,
            9,
            2_000,
            200_000,
            decks.clone(),
            [PauperMulliganModeV1::Learned; 2],
        )
        .unwrap()
    };
    let mut clean = fresh();
    assert!(clean
        .set_runtime_rules_v1(RuntimeRulesV1::RESOLUTION_BOUNDARY_V1)
        .is_ok());
    for (owner, zone) in [(PlayerId::P0, Zone::Library), (PlayerId::P1, Zone::Hand)] {
        let mut session = fresh();
        put(&mut session.state, owner, "Gurmag Angler", zone);
        let refused = session
            .set_runtime_rules_v1(RuntimeRulesV1::RESOLUTION_BOUNDARY_V1)
            .unwrap_err();
        assert!(refused.contains("Delve"), "{refused}");
        assert!(session
            .set_runtime_rules_v1(RuntimeRulesV1::default())
            .is_ok());
    }
}
