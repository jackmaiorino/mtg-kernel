//! Integration coverage for the spellbench/v1 environment bridge: the
//! in-process server contract and the `agent_bridge_v1` binary over real
//! stdio, including a driven Burn mirror, the closed error-code table,
//! single-entry retry idempotence, and combat inclusion-scan group integrity.

use mtg_kernel::agent_bridge_v1::{
    SpellbenchBridgeServerV1, SpellbenchEngineIdentityV1, SPELLBENCH_PROTOCOL_V1,
};
use mtg_kernel::rl_session::{RlEpisodeSessionV1, RlSessionResponseV1};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const BURN_MIRROR_MAX_DECISIONS: u64 = 8192;
const BURN_MIRROR_MAX_STEPS: u64 = 1_048_576;

fn test_server() -> SpellbenchBridgeServerV1 {
    SpellbenchBridgeServerV1::with_engine_identity(
        SpellbenchEngineIdentityV1::with_source_revision(None),
    )
}

fn hello_line(request_id: &str) -> String {
    json!({
        "request_type": "hello",
        "protocol": SPELLBENCH_PROTOCOL_V1,
        "request_id": request_id,
    })
    .to_string()
}

fn burn_mirror_reset_line(request_id: &str, game_id: &str, game_seed: u64) -> String {
    json!({
        "request_type": "reset",
        "protocol": SPELLBENCH_PROTOCOL_V1,
        "request_id": request_id,
        "game_id": game_id,
        "format": "pauper-bo1",
        "seats": [
            {"seat": "p0", "deck": {"catalog_id": "Burn"}},
            {"seat": "p1", "deck": {"catalog_id": "Burn"}},
        ],
        "game_seed": game_seed,
        "max_decisions": BURN_MIRROR_MAX_DECISIONS,
        "max_steps": BURN_MIRROR_MAX_STEPS,
    })
    .to_string()
}

fn step_line_for(request_id: &str, decision: &Value, candidate_index: usize) -> String {
    let candidate = &decision["candidates"].as_array().unwrap()[candidate_index];
    json!({
        "request_type": "step",
        "protocol": SPELLBENCH_PROTOCOL_V1,
        "request_id": request_id,
        "game_id": decision["game_id"],
        "expected_step": decision["step"],
        "selection": {
            "candidate_id": candidate["candidate_id"],
            "semantic_echo": candidate["semantic"],
        },
    })
    .to_string()
}

fn parse_line(line: &str) -> Value {
    serde_json::from_str(line).expect("response line is valid JSON")
}

/// Independent recomputation of `candidates_sha256`: serde_json's default map
/// ordering is code-point-sorted and its serializer is compact, so
/// `to_string` of the reduced array is the spec-canonical encoding.
fn expected_candidates_sha256(decision: &Value) -> String {
    let reduced: Vec<Value> = decision["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|candidate| {
            json!({
                "candidate_id": candidate["candidate_id"],
                "semantic": candidate["semantic"],
            })
        })
        .collect();
    let canonical = serde_json::to_string(&reduced).unwrap();
    format!("{:x}", Sha256::digest(canonical.as_bytes()))
}

/// Parses one raw kernel v5 payload (`observation` or `legal_actions`) from
/// the `x_kernel_v5` extension, which carries it as exact JSON text.
fn kernel_v5_payload(decision: &Value, field: &str) -> Value {
    let text = decision["extensions"]["x_kernel_v5"][format!("{field}_json")]
        .as_str()
        .unwrap_or_else(|| panic!("x_kernel_v5.{field}_json is a string"));
    serde_json::from_str(text).expect("x_kernel_v5 payload is valid JSON")
}

/// Asserts every decision invariant a host may rely on, and returns the
/// candidate index a deterministic "attack when offered" policy selects.
fn assert_decision_shape(decision: &Value, expected_step: u64) -> usize {
    assert_eq!(decision["response_type"], "decision");
    assert_eq!(decision["protocol"], "spellbench/v1");
    assert_eq!(decision["step"], expected_step);
    let acting = decision["acting_seat"].as_str().unwrap();
    assert!(matches!(acting, "p0" | "p1"));
    let group = &decision["group"];
    assert!(group["group_id"].is_u64());
    let substep_index = group["substep_index"].as_u64().unwrap();
    let substep_count = group["substep_count"].as_u64().unwrap();
    assert!(substep_count >= 1 && substep_index < substep_count);
    let summary = &decision["state_summary"];
    for key in [
        "turn",
        "phase_step",
        "active_seat",
        "priority_seat",
        "seats",
        "stack_count",
    ] {
        assert!(summary.get(key).is_some(), "state_summary missing {key}");
    }
    let candidates = decision["candidates"].as_array().unwrap();
    assert!(!candidates.is_empty());
    for (index, candidate) in candidates.iter().enumerate() {
        assert_eq!(candidate["candidate_id"], index as u64);
        let semantic = &candidate["semantic"];
        let kind = semantic["kind"].as_str().unwrap();
        assert_ne!(
            kind, "ambiguous",
            "the refused variant must never be emitted"
        );
        assert!(candidate.get("display_text").is_some());
    }
    assert_eq!(
        decision["candidates_sha256"].as_str().unwrap(),
        expected_candidates_sha256(decision)
    );
    assert_eq!(
        decision["provenance"],
        json!({
            "engine_name": "mtg-kernel",
            "engine_version": mtg_kernel::KERNEL_VERSION,
            "rules_snapshot_id": decision["provenance"]["rules_snapshot_id"],
            "card_pool_identity": decision["provenance"]["card_pool_identity"],
        })
    );
    assert_eq!(
        kernel_v5_payload(decision, "legal_actions")
            .as_array()
            .unwrap()
            .len(),
        candidates.len()
    );
    assert_eq!(
        kernel_v5_payload(decision, "observation")["step_index"]
            .as_u64()
            .unwrap(),
        expected_step
    );
    // Policy: attack with everything offered, block with everything offered;
    // otherwise take candidate 0.
    candidates
        .iter()
        .position(|candidate| {
            candidate["semantic"]["kind"] == "choose_attacker_inclusion"
                && candidate["semantic"]["include"] == true
        })
        .or_else(|| {
            candidates.iter().position(|candidate| {
                candidate["semantic"]["kind"] == "choose_blocker_inclusion"
                    && candidate["semantic"]["include"] == true
            })
        })
        .unwrap_or(0)
}

/// Drives a full Burn mirror through any handle_line-like sink, checking
/// group/substep integrity and semantic round-trips on every step. Returns
/// the terminal response and whether a decomposed combat scan appeared.
fn drive_burn_mirror(
    mut exchange: impl FnMut(&str) -> Value,
    game_id: &str,
    game_seed: u64,
) -> (Value, bool) {
    let mut current = exchange(&burn_mirror_reset_line("r-0", game_id, game_seed));
    let mut steps = 0_u64;
    let mut next_group_id = 0_u64;
    let mut open_group: Option<(u64, u64, u64)> = None;
    let mut saw_combat_scan = false;
    loop {
        match current["response_type"].as_str().unwrap() {
            "decision" => {
                let group = &current["group"];
                let group_id = group["group_id"].as_u64().unwrap();
                let substep_index = group["substep_index"].as_u64().unwrap();
                let substep_count = group["substep_count"].as_u64().unwrap();
                match open_group {
                    None => {
                        assert_eq!(substep_index, 0, "a group opens at substep 0");
                        assert_eq!(group_id, next_group_id, "group ids are dense");
                    }
                    Some((open_id, expected_index, open_count)) => {
                        assert_eq!(group_id, open_id, "group id is pinned within a group");
                        assert_eq!(substep_index, expected_index, "substeps run dense");
                        assert_eq!(substep_count, open_count, "substep_count is fixed");
                    }
                }
                if substep_count > 1 {
                    saw_combat_scan = true;
                }
                if substep_index + 1 == substep_count {
                    open_group = None;
                    next_group_id += 1;
                } else {
                    open_group = Some((group_id, substep_index + 1, substep_count));
                }
                let chosen = assert_decision_shape(&current, steps);
                let step = step_line_for(&format!("s-{steps}"), &current, chosen);
                current = exchange(&step);
                steps += 1;
                assert!(steps <= BURN_MIRROR_MAX_STEPS);
            }
            "terminal" => {
                assert_eq!(current["step_count"], steps);
                assert!(
                    open_group.is_none(),
                    "a terminal must not interrupt a group"
                );
                return (current, saw_combat_scan);
            }
            other => panic!("unexpected response type {other}: {current}"),
        }
    }
}

/// Spec Section 2: every protocol number is an integer with `|x| <= 2^53`.
const SPEC_MAX_SAFE_INTEGER: u64 = 1 << 53;

fn assert_spec_safe_integers(value: &Value, path: &str) {
    match value {
        Value::Number(number) => {
            let within = if let Some(unsigned) = number.as_u64() {
                unsigned <= SPEC_MAX_SAFE_INTEGER
            } else if let Some(signed) = number.as_i64() {
                signed.unsigned_abs() <= SPEC_MAX_SAFE_INTEGER
            } else {
                false
            };
            assert!(within, "{path} = {number} is not a spec-safe integer");
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                assert_spec_safe_integers(item, &format!("{path}[{index}]"));
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                assert_spec_safe_integers(item, &format!("{path}.{key}"));
            }
        }
        _ => {}
    }
}

#[test]
fn agent_bridge_emits_only_spec_safe_integers() {
    let mut server = test_server();
    let hello = parse_line(&server.handle_line(&hello_line("h-0")));
    assert_spec_safe_integers(&hello, "hello");
    let (terminal, _) = drive_burn_mirror(
        |line| {
            let response = parse_line(&server.handle_line(line));
            assert_spec_safe_integers(&response, "response");
            response
        },
        "g-safe-integers",
        5151,
    );
    assert_eq!(terminal["response_type"], "terminal");
}

/// Catalog decks policy surface v5 can play through (Terror is refused).
const SWEEP_DECKS: [&str; 8] = [
    "Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "CawGates", "Faeries",
];

fn splitmix64(state: u64) -> u64 {
    let mut z = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Deterministic test policy: a pseudo-random candidate per (seed, step).
fn sweep_choice(seed: u64, step: u64, candidate_count: usize) -> usize {
    (splitmix64(seed ^ step.wrapping_mul(0xD1B5_4A32_D192_ED03)) % candidate_count as u64) as usize
}

fn reset_line_for(request_id: &str, game_id: &str, decks: [&str; 2], game_seed: u64) -> String {
    json!({
        "request_type": "reset",
        "protocol": SPELLBENCH_PROTOCOL_V1,
        "request_id": request_id,
        "game_id": game_id,
        "format": "pauper-bo1",
        "seats": [
            {"seat": "p0", "deck": {"catalog_id": decks[0]}},
            {"seat": "p1", "deck": {"catalog_id": decks[1]}},
        ],
        "game_seed": game_seed,
        "max_decisions": 600,
        "max_steps": 6000,
    })
    .to_string()
}

/// FNV-1a 64 over the game id's UTF-8 bytes (`fnv1a64-utf8-game-id/v1`).
fn episode_id_for(game_id: &str) -> u64 {
    game_id.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

type ObjectKey = (u64, u64);

fn collect_named_cards(node: &Value, named: &mut HashMap<ObjectKey, String>) {
    match node {
        Value::Object(map) => {
            if let (Some(stable), Some(Value::String(name))) =
                (map.get("stable"), map.get("card_name"))
            {
                let key = (
                    stable["arena_id"].as_u64().unwrap(),
                    stable["zone_change_count"].as_u64().unwrap(),
                );
                if let Some(previous) = named.insert(key, name.clone()) {
                    assert_eq!(
                        &previous, name,
                        "observation names {key:?} twice differently"
                    );
                }
            }
            map.values()
                .for_each(|child| collect_named_cards(child, named));
        }
        Value::Array(items) => items
            .iter()
            .for_each(|child| collect_named_cards(child, named)),
        _ => {}
    }
}

/// What the acting seat's own observation reveals: the name it shows for each
/// object, and the objects its pending effect offers as targets.
fn acting_seat_knowledge(observation: &Value) -> (HashMap<ObjectKey, String>, HashSet<ObjectKey>) {
    let mut named = HashMap::new();
    collect_named_cards(observation, &mut named);
    let mut offered = HashSet::new();
    let choice = &observation["projection"]["engine_context"]["pending_effect"]["choice"];
    if choice["choice_kind"] == "targets" {
        for key in ["legal_targets", "selected_targets"] {
            for target in choice[key].as_array().unwrap() {
                if let Some(object) = target.get("object") {
                    offered.insert((
                        object["arena_id"].as_u64().unwrap(),
                        object["zone_change_count"].as_u64().unwrap(),
                    ));
                }
            }
        }
    }
    (named, offered)
}

fn collect_object_refs<'a>(node: &'a Value, refs: &mut Vec<&'a Value>) {
    match node {
        Value::Object(map) => {
            if map.contains_key("object_id") && map.contains_key("zone") {
                refs.push(node);
            }
            map.values()
                .for_each(|child| collect_object_refs(child, refs));
        }
        Value::Array(items) => items
            .iter()
            .for_each(|child| collect_object_refs(child, refs)),
        _ => {}
    }
}

fn object_key(object_ref: &Value) -> ObjectKey {
    // obj-<arena_id:06>-z<zone_change_count:04>
    let id = object_ref["object_id"].as_str().unwrap();
    let (arena, zone_changes) = id.strip_prefix("obj-").unwrap().split_once("-z").unwrap();
    (arena.parse().unwrap(), zone_changes.parse().unwrap())
}

/// Spec Section 5: `card_name` is the name the observing seat knows, null
/// only for identities hidden from it.
fn assert_candidate_names_follow_the_observation(decision: &Value) -> usize {
    let (named, offered) = acting_seat_knowledge(&kernel_v5_payload(decision, "observation"));
    let acting = decision["acting_seat"].as_str().unwrap();
    let mut refs = Vec::new();
    for candidate in decision["candidates"].as_array().unwrap() {
        collect_object_refs(&candidate["semantic"], &mut refs);
    }
    for object_ref in &refs {
        let key = object_key(object_ref);
        let shown = &object_ref["card_name"];
        match named.get(&key) {
            Some(name) => assert_eq!(shown, name, "name the observation gives {object_ref}"),
            None if offered.contains(&key) => {
                assert!(
                    shown.is_string(),
                    "offered target must be named: {object_ref}"
                )
            }
            None => {
                let zone = object_ref["zone"].as_str().unwrap();
                let hidden = zone == "library"
                    || (zone == "hand" && object_ref["owner_seat"].as_str().unwrap() != acting);
                assert_eq!(shown.is_null(), hidden, "visibility of {object_ref}");
            }
        }
    }
    refs.len()
}

#[test]
fn agent_bridge_names_what_the_acting_seat_knows_and_counts_every_terminal() {
    let mut checked_refs = 0;
    for deck in SWEEP_DECKS {
        for seed in 1..=3_u64 {
            let mut server = test_server();
            let game_id = format!("g-sweep-{deck}-{seed}");
            let mut current = parse_line(&server.handle_line(&reset_line_for(
                "r-0",
                &game_id,
                [deck, deck],
                seed,
            )));
            let mut last: Option<Value> = None;
            while current["response_type"] == "decision" {
                checked_refs += assert_candidate_names_follow_the_observation(&current);
                let step = current["step"].as_u64().unwrap();
                let choice =
                    sweep_choice(seed, step, current["candidates"].as_array().unwrap().len());
                let next = parse_line(&server.handle_line(&step_line_for(
                    &format!("s-{step}"),
                    &current,
                    choice,
                )));
                last = Some(current);
                current = next;
            }
            assert_eq!(
                current["response_type"], "terminal",
                "{deck} seed {seed}: {current}"
            );
            // The reference host's count rule: every answered decision is a
            // step, and a physical decision counts once its group completes.
            let (steps, decisions) = match &last {
                None => (0, 0),
                Some(answered) => {
                    let group = &answered["group"];
                    let closed = group["substep_index"].as_u64().unwrap() + 1
                        == group["substep_count"].as_u64().unwrap();
                    (
                        answered["step"].as_u64().unwrap() + 1,
                        group["group_id"].as_u64().unwrap() + u64::from(closed),
                    )
                }
            };
            assert_eq!(
                current["step_count"], steps,
                "{deck} seed {seed}: {current}"
            );
            assert_eq!(
                current["decision_count"], decisions,
                "{deck} seed {seed}: {current}"
            );
        }
    }
    assert!(
        checked_refs > 10_000,
        "sweep checked only {checked_refs} references"
    );
}

#[test]
fn agent_bridge_applies_exactly_the_chosen_kernel_action() {
    // Differential: a raw kernel session stepped in lockstep with the bridge
    // must see the same observation and legal actions at every step.
    for deck in ["Burn", "Faeries", "CawGates", "Spy"] {
        let seed = 42;
        let game_id = format!("g-diff-{deck}");
        let mut server = test_server();
        let mut session = RlEpisodeSessionV1::reset_with_decks_and_limits(
            episode_id_for(&game_id),
            seed,
            600,
            6000,
            [deck.to_string(), deck.to_string()],
        )
        .unwrap();
        let mut current =
            parse_line(&server.handle_line(&reset_line_for("r-0", &game_id, [deck, deck], seed)));
        let mut kernel = session.current_response();
        loop {
            match (&kernel, current["response_type"].as_str().unwrap()) {
                (RlSessionResponseV1::Decision(expected), "decision") => {
                    let extension = &current["extensions"]["x_kernel_v5"];
                    assert_eq!(
                        extension["observation_json"].as_str().unwrap(),
                        serde_json::to_string(&expected.observation).unwrap()
                    );
                    assert_eq!(
                        extension["legal_actions_json"].as_str().unwrap(),
                        serde_json::to_string(&expected.legal_actions).unwrap()
                    );
                    let step = expected.step;
                    let choice = sweep_choice(seed, step, expected.legal_actions.len());
                    let stable_id = expected.legal_actions[choice].stable_id.clone();
                    let request = step_line_for(&format!("s-{step}"), &current, choice);
                    current = parse_line(&server.handle_line(&request));
                    kernel = session
                        .step(episode_id_for(&game_id), step, choice as u32, &stable_id)
                        .unwrap();
                }
                (RlSessionResponseV1::Terminal(expected), "terminal") => {
                    assert_eq!(current["step_count"], expected.policy_step_count, "{deck}");
                    break;
                }
                (_, other) => {
                    panic!("{deck}: bridge answered {other} where the kernel did not: {current}")
                }
            }
        }
    }
}

/// Plays one mirror under the sweep policy through the in-process bridge.
/// Returns the terminal response line, the last answered decision, and the
/// candidate chosen for it.
fn drive_sweep_game(deck: &str, seed: u64, game_id: &str) -> (String, Value, usize) {
    let mut server = test_server();
    let mut line = server.handle_line(&reset_line_for("r-0", game_id, [deck, deck], seed));
    let mut answered = None;
    loop {
        let current = parse_line(&line);
        if current["response_type"] != "decision" {
            let (decision, choice) = answered.expect("the game answers at least one decision");
            return (line, decision, choice);
        }
        let step = current["step"].as_u64().unwrap();
        let choice = sweep_choice(seed, step, current["candidates"].as_array().unwrap().len());
        line = server.handle_line(&step_line_for(&format!("s-{step}"), &current, choice));
        answered = Some((current, choice));
    }
}

/// Reproduces a kernel defect (Elves halts of the pauper-kernel launch
/// benchmark): policy surface v5's attacker scan offers `include: false` for
/// a goaded creature the engine requires to attack, so once it is declined
/// no answer to the scan's last substep is legal, and the session refuses
/// the answer the bridge validated. The halted reason names the session
/// error code and carries the kernel's message, byte-identical across runs
/// (the arena hashes its ledger). If a kernel fix lets this game finish,
/// pin the reason on another post-validation rejection.
#[test]
fn agent_bridge_names_the_session_error_when_a_validated_step_is_refused() {
    let (line, answered, choice) = drive_sweep_game("Elves", 11, "g-goad-scan");
    let terminal = parse_line(&line);
    assert_eq!(terminal["outcome"], "halted", "{terminal}");
    assert_eq!(terminal["classification"], "halted");
    assert_eq!(terminal["winner"], Value::Null);
    assert_eq!(
        terminal["reason"],
        "engine_contract_failure:session_step_rejected_after_validation:\
         stale_environment_binding:selected action no longer matches the active policy environment"
    );
    // The refused answer closes an attacker inclusion scan at step 99.
    assert_eq!(answered["step"], 99);
    assert_eq!(
        answered["candidates"][choice]["semantic"]["kind"],
        "choose_attacker_inclusion"
    );
    let group = &answered["group"];
    assert_eq!(
        group["substep_index"].as_u64().unwrap() + 1,
        group["substep_count"].as_u64().unwrap()
    );
    assert_eq!(terminal["step_count"], 100);
    assert_eq!(
        terminal["decision_count"],
        group["group_id"].as_u64().unwrap() + 1
    );
    let (again, _, _) = drive_sweep_game("Elves", 11, "g-goad-scan");
    assert_eq!(again, line, "the halted terminal is deterministic");
}

#[test]
fn agent_bridge_serves_sequential_games_with_fresh_game_ids() {
    // Spec Section 2: at most one ACTIVE game per process; game ids are
    // unique per process.
    let mut server = test_server();
    let capped = |request_id: &str, game_id: &str| {
        json!({
            "request_type": "reset",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": request_id,
            "game_id": game_id,
            "format": "pauper-bo1",
            "seats": [
                {"seat": "p0", "deck": {"catalog_id": "Burn"}},
                {"seat": "p1", "deck": {"catalog_id": "Burn"}},
            ],
            "game_seed": 1,
            "max_decisions": 0,
            "max_steps": 0,
        })
        .to_string()
    };
    let first = parse_line(&server.handle_line(&capped("r-1", "g-1")));
    assert_eq!(first["response_type"], "terminal");
    let second = parse_line(&server.handle_line(&capped("r-2", "g-2")));
    assert_eq!(
        second["response_type"], "terminal",
        "a finished game frees the process"
    );
    let reused = parse_line(&server.handle_line(&capped("r-3", "g-1")));
    assert_eq!(
        reused["error"]["code"], "malformed_request",
        "game ids are unique per process"
    );
    let live = parse_line(&server.handle_line(&reset_line_for("r-4", "g-3", ["Burn", "Burn"], 5)));
    assert_eq!(live["response_type"], "decision");
    let overlapping = parse_line(&server.handle_line(&capped("r-5", "g-4")));
    assert_eq!(overlapping["error"]["code"], "game_already_active");
}

#[test]
fn agent_bridge_errors_echo_an_empty_request_id_when_none_is_readable() {
    // Spec Section 4.1: every response carries a string request_id.
    let mut server = test_server();
    for line in [
        "not json at all",
        "[1,2,3]",
        r#"{"request_type":"hello","protocol":"spellbench/v1","request_id":7}"#,
    ] {
        let response = parse_line(&server.handle_line(line));
        assert_eq!(response["response_type"], "error", "{line}");
        assert_eq!(response["request_id"], "", "{line}");
    }
}

#[test]
fn agent_bridge_refuses_decks_policy_v5_cannot_play() {
    // Terror stages Escape, which policy surface v5 cannot represent: most of
    // its games would halt, biasing any rating built from the rest.
    let mut server = test_server();
    let response =
        parse_line(&server.handle_line(&reset_line_for("r-0", "g-terror", ["Terror", "Burn"], 1)));
    assert_eq!(response["error"]["code"], "unsupported_deck");
}

#[test]
fn agent_bridge_in_process_burn_mirror_reaches_terminal_with_clean_groups() {
    let mut server = test_server();
    let hello = parse_line(&server.handle_line(&hello_line("h-0")));
    assert_eq!(hello["response_type"], "hello_ok");
    let (terminal, saw_combat_scan) = drive_burn_mirror(
        |line| parse_line(&server.handle_line(line)),
        "g-burn-mirror",
        5151,
    );
    assert!(
        matches!(
            terminal["outcome"].as_str().unwrap(),
            "p0_win" | "p1_win" | "draw" | "truncated"
        ),
        "unexpected terminal: {terminal}"
    );
    assert_eq!(
        terminal["classification"].as_str().unwrap(),
        match terminal["outcome"].as_str().unwrap() {
            "p0_win" | "p1_win" | "draw" => "natural",
            _ => "truncated",
        }
    );
    assert!(
        saw_combat_scan,
        "attacking Burn mirror must reach combat scans"
    );
}

#[test]
fn agent_bridge_semantics_round_trip_both_directions() {
    // Forward: every emitted candidate semantic is spec-shaped and hashed.
    // Reverse: a non-zero candidate is accepted and a wrong echo is refused;
    // agent_bridge_applies_exactly_the_chosen_kernel_action proves the
    // selected kernel action is applied.
    let mut server = test_server();
    let current = parse_line(&server.handle_line(&burn_mirror_reset_line("r-0", "g-rt", 99)));
    assert_eq!(current["response_type"], "decision");
    let candidate_count = current["candidates"].as_array().unwrap().len();
    assert!(
        candidate_count >= 2,
        "Burn opener should offer a real choice"
    );
    // Answer with the LAST candidate to prove non-zero indices bind exactly.
    let chosen = candidate_count - 1;
    let step = step_line_for("s-0", &current, chosen);
    let next = parse_line(&server.handle_line(&step));
    assert_ne!(next["response_type"], "error", "echo of candidate {chosen}");
    // The engine advanced exactly one step; its next decision (or terminal)
    // reflects that selection. A different semantic with the same id fails.
    if next["response_type"] == "decision" {
        let mut wrong = next["candidates"][0]["semantic"].clone();
        wrong["kind"] = json!("pass_wrong");
        let bad = json!({
            "request_type": "step",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "s-1",
            "game_id": "g-rt",
            "expected_step": next["step"],
            "selection": {"candidate_id": 0, "semantic_echo": wrong},
        })
        .to_string();
        let rejected = parse_line(&server.handle_line(&bad));
        assert_eq!(rejected["error"]["code"], "semantic_echo_mismatch");
    }
}

struct BridgeProcess {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
}

impl BridgeProcess {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_agent_bridge_v1"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn agent_bridge_v1");
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        BridgeProcess {
            child,
            stdin,
            reader: BufReader::new(stdout),
        }
    }

    fn exchange(&mut self, line: &str) -> Value {
        writeln!(self.stdin, "{line}").unwrap();
        self.stdin.flush().unwrap();
        let mut response = String::new();
        self.reader.read_line(&mut response).unwrap();
        parse_line(response.trim_end())
    }

    fn finish(mut self) {
        drop(self.stdin);
        let status = self.child.wait().unwrap();
        assert!(status.success());
        let mut stderr = String::new();
        self.child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        assert!(stderr.trim().is_empty(), "unexpected stderr: {stderr}");
    }
}

#[test]
fn agent_bridge_process_hello_reset_step_and_clean_shutdown() {
    let mut process = BridgeProcess::spawn();
    let hello = process.exchange(&hello_line("h-1"));
    assert_eq!(hello["response_type"], "hello_ok");
    assert_eq!(hello["engine"]["name"], "mtg-kernel");
    assert_eq!(hello["protocol"], "spellbench/v1");

    let first = process.exchange(&burn_mirror_reset_line("r-1", "g-proc", 5151));
    assert_eq!(first["response_type"], "decision");
    assert_eq!(first["step"], 0);

    let step = step_line_for("s-1", &first, 0);
    let next = process.exchange(&step);
    assert_ne!(next["response_type"], "error");
    process.finish();
}

#[test]
fn agent_bridge_process_full_burn_mirror_over_stdio() {
    let mut process = BridgeProcess::spawn();
    let (terminal, saw_combat_scan) =
        drive_burn_mirror(|line| process.exchange(line), "g-proc-full", 5151);
    let step_count = terminal["step_count"].as_u64().unwrap();
    let decision_count = terminal["decision_count"].as_u64().unwrap();
    assert!(step_count > 0 && decision_count > 0);
    assert!(
        decision_count <= step_count,
        "physical decisions never exceed wire steps"
    );
    assert!(saw_combat_scan, "process game must reach combat scans");
    process.finish();
}

#[test]
fn agent_bridge_process_error_table_retry_and_framing() {
    let mut process = BridgeProcess::spawn();

    // malformed_json: not JSON at all.
    writeln!(process.stdin, "not json at all").unwrap();
    process.stdin.flush().unwrap();
    let mut line = String::new();
    process.reader.read_line(&mut line).unwrap();
    assert_eq!(
        parse_line(line.trim_end())["error"]["code"],
        "malformed_json"
    );

    // malformed_json: duplicate object keys.
    writeln!(
        process.stdin,
        "{{\"request_type\":\"hello\",\"request_type\":\"hello\"}}"
    )
    .unwrap();
    process.stdin.flush().unwrap();
    line.clear();
    process.reader.read_line(&mut line).unwrap();
    assert_eq!(
        parse_line(line.trim_end())["error"]["code"],
        "malformed_json"
    );

    // malformed_request: top-level array.
    let response = process.exchange("[1,2,3]");
    assert_eq!(response["error"]["code"], "malformed_request");

    // protocol_mismatch.
    let response = process.exchange(
        &json!({
            "request_type": "hello",
            "protocol": "spellbench/v0",
            "request_id": "p-1",
        })
        .to_string(),
    );
    assert_eq!(response["error"]["code"], "protocol_mismatch");

    // step_before_reset.
    let response = process.exchange(
        &json!({
            "request_type": "step",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "s-early",
            "game_id": "g-none",
            "expected_step": 0,
            "selection": {"candidate_id": 0, "semantic_echo": {"kind": "pass"}},
        })
        .to_string(),
    );
    assert_eq!(response["error"]["code"], "step_before_reset");

    // unsupported_format.
    let response = process.exchange(
        &burn_mirror_reset_line("r-fmt", "g-fmt", 1).replace("pauper-bo1", "vintage-bo1"),
    );
    assert_eq!(response["error"]["code"], "unsupported_format");

    // unsupported_deck: decklists-as-data refused.
    let response = process.exchange(
        &json!({
            "request_type": "reset",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "r-decklist",
            "game_id": "g-decklist",
            "format": "pauper-bo1",
            "seats": [
                {"seat": "p0", "deck": {"decklist": [{"name": "Mountain", "count": 60}]}},
                {"seat": "p1", "deck": {"catalog_id": "Burn"}},
            ],
            "game_seed": 1,
            "max_decisions": 16,
            "max_steps": 2048,
        })
        .to_string(),
    );
    assert_eq!(response["error"]["code"], "unsupported_deck");

    // unsupported_deck: unknown catalog id.
    let response = process.exchange(
        &burn_mirror_reset_line("r-unknown", "g-unknown", 1).replace("\"Burn\"", "\"NotADeck\""),
    );
    assert_eq!(response["error"]["code"], "unsupported_deck");

    // A real game, then every in-game error.
    let decision = process.exchange(&burn_mirror_reset_line("r-1", "g-errors", 5151));
    assert_eq!(decision["response_type"], "decision");

    // Retry: identical bytes return the cached response with no side effects.
    let step = step_line_for("s-1", &decision, 0);
    let first = process.exchange(&step);
    let replayed = process.exchange(&step);
    assert_eq!(replayed, first);
    let mut current = first;

    // request_id_reuse_mismatch: same id, different payload.
    let reuse =
        step_line_for("s-1", &decision, 0).replace("\"expected_step\":0", "\"expected_step\":1");
    let response = process.exchange(&reuse);
    assert_eq!(response["error"]["code"], "request_id_reuse_mismatch");

    if current["response_type"] == "decision" {
        // game_id_mismatch.
        let wrong_game = step_line_for("s-2", &current, 0).replace(
            &format!("\"game_id\":\"{}\"", current["game_id"].as_str().unwrap()),
            "\"game_id\":\"g-other\"",
        );
        let response = process.exchange(&wrong_game);
        assert_eq!(response["error"]["code"], "game_id_mismatch");

        // expected_step_mismatch.
        let wrong_step = json!({
            "request_type": "step",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "s-3",
            "game_id": current["game_id"],
            "expected_step": current["step"].as_u64().unwrap() + 7,
            "selection": {
                "candidate_id": 0,
                "semantic_echo": current["candidates"][0]["semantic"],
            },
        })
        .to_string();
        let response = process.exchange(&wrong_step);
        assert_eq!(response["error"]["code"], "expected_step_mismatch");

        // candidate_id_out_of_range.
        let out_of_range = json!({
            "request_type": "step",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "s-4",
            "game_id": current["game_id"],
            "expected_step": current["step"],
            "selection": {
                "candidate_id": current["candidates"].as_array().unwrap().len() as u64,
                "semantic_echo": current["candidates"][0]["semantic"],
            },
        })
        .to_string();
        let response = process.exchange(&out_of_range);
        assert_eq!(response["error"]["code"], "candidate_id_out_of_range");

        // semantic_echo_mismatch.
        let mut wrong_echo = current["candidates"][0]["semantic"].clone();
        wrong_echo["kind"] = json!("not_the_kind");
        let wrong_echo_step = json!({
            "request_type": "step",
            "protocol": SPELLBENCH_PROTOCOL_V1,
            "request_id": "s-5",
            "game_id": current["game_id"],
            "expected_step": current["step"],
            "selection": {"candidate_id": 0, "semantic_echo": wrong_echo},
        })
        .to_string();
        let response = process.exchange(&wrong_echo_step);
        assert_eq!(response["error"]["code"], "semantic_echo_mismatch");

        // The game is still live after every rejected step.
        current = process.exchange(&step_line_for("s-6", &current, 0));
        assert_ne!(current["response_type"], "error");
    }

    // Drive to terminal, then game_already_terminal and game_already_active.
    let mut steps = current["step"].as_u64().unwrap_or(0);
    let terminal = loop {
        match current["response_type"].as_str().unwrap() {
            "decision" => {
                let chosen = assert_decision_shape(&current, steps);
                current = process.exchange(&step_line_for(&format!("t-{steps}"), &current, chosen));
                steps += 1;
            }
            "terminal" => break current,
            other => panic!("unexpected response type {other}"),
        }
    };
    let late_step = json!({
        "request_type": "step",
        "protocol": SPELLBENCH_PROTOCOL_V1,
        "request_id": "s-late",
        "game_id": terminal["game_id"],
        "expected_step": terminal["step_count"],
        "selection": {"candidate_id": 0, "semantic_echo": {"kind": "pass"}},
    })
    .to_string();
    let response = process.exchange(&late_step);
    assert_eq!(response["error"]["code"], "game_already_terminal");

    // A finished game frees the process for the next game id.
    let reentry = process.exchange(&burn_mirror_reset_line("r-2", "g-again", 1));
    assert_eq!(reentry["response_type"], "decision");

    // \r\n is tolerated on read.
    write!(process.stdin, "{}\r\n", hello_line("h-crlf")).unwrap();
    process.stdin.flush().unwrap();
    line.clear();
    process.reader.read_line(&mut line).unwrap();
    assert_eq!(parse_line(line.trim_end())["response_type"], "hello_ok");

    // An oversized line is rejected and framing resynchronizes.
    let oversized = " ".repeat(8 * 1024 * 1024 + 16);
    writeln!(process.stdin, "{oversized}").unwrap();
    process.stdin.flush().unwrap();
    line.clear();
    process.reader.read_line(&mut line).unwrap();
    assert_eq!(
        parse_line(line.trim_end())["error"]["code"],
        "malformed_json"
    );

    // Invalid UTF-8 is malformed_json, and the process still serves requests.
    process.stdin.write_all(b"\xff\xfe not utf-8\n").unwrap();
    process.stdin.flush().unwrap();
    line.clear();
    process.reader.read_line(&mut line).unwrap();
    assert_eq!(
        parse_line(line.trim_end())["error"]["code"],
        "malformed_json"
    );

    let response = process.exchange(&hello_line("h-after-framing"));
    assert_eq!(response["response_type"], "hello_ok");

    process.finish();
}

#[test]
fn agent_bridge_process_terminal_counts_match_driven_steps() {
    // A capped game truncates deterministically: decision_count equals the
    // cap and step_count equals the number of answered wire decisions.
    let mut process = BridgeProcess::spawn();
    let reset = json!({
        "request_type": "reset",
        "protocol": SPELLBENCH_PROTOCOL_V1,
        "request_id": "r-cap",
        "game_id": "g-cap",
        "format": "pauper-bo1",
        "seats": [
            {"seat": "p0", "deck": {"catalog_id": "Burn"}},
            {"seat": "p1", "deck": {"catalog_id": "Burn"}},
        ],
        "game_seed": 5151,
        "max_decisions": 8,
        "max_steps": 1024,
    })
    .to_string();
    let mut current = process.exchange(&reset);
    let mut steps = 0_u64;
    loop {
        match current["response_type"].as_str().unwrap() {
            "decision" => {
                current = process.exchange(&step_line_for(&format!("s-{steps}"), &current, 0));
                steps += 1;
            }
            "terminal" => break,
            other => panic!("unexpected response type {other}"),
        }
    }
    assert_eq!(current["outcome"], "truncated");
    assert_eq!(current["classification"], "truncated");
    assert_eq!(current["winner"], Value::Null);
    assert_eq!(current["step_count"], steps);
    assert_eq!(current["decision_count"], 8);
    process.finish();
}

/// Re-pinning helper after an engine change: prints every halted sweep game.
#[test]
#[ignore = "seed search helper; run with --ignored --nocapture"]
fn search_halted_sweep_games() {
    for deck in [
        "Elves", "CawGates", "Faeries", "Affinity", "Rally", "Wildfire", "Burn", "Spy",
    ] {
        for seed in 1..=200_u64 {
            let (line, answered, choice) = drive_sweep_game(deck, seed, "g-search");
            let terminal = parse_line(&line);
            if terminal["outcome"] == "halted" {
                println!(
                    "HALT deck={deck} seed={seed} step={} kind={} step_count={} reason={}",
                    answered["step"],
                    answered["candidates"][choice]["semantic"]["kind"],
                    terminal["step_count"],
                    terminal["reason"]
                );
            }
        }
    }
}

#[test]
fn agent_bridge_process_flag_enables_x_kernel_flat_v4() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    let mut child = Command::new(env!("CARGO_BIN_EXE_agent_bridge_v1"))
        .arg("--x-kernel-flat-v4")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut exchange = |line: String| -> Value {
        writeln!(stdin, "{line}").unwrap();
        let mut response = String::new();
        stdout.read_line(&mut response).unwrap();
        serde_json::from_str(&response).unwrap()
    };
    let hello = exchange(hello_line("h-1"));
    assert_eq!(
        hello["extensions"],
        json!(["x_kernel_v5", "x_kernel_flat_v4"])
    );
    let decision = exchange(burn_mirror_reset_line("r-1", "g-flag", 12345));
    assert_eq!(
        decision["extensions"]["x_kernel_flat_v4"]["schema"],
        "mtg-kernel-spellbench-flat-v4/v1"
    );
    drop(stdin);
    assert!(child.wait().unwrap().success());
    let refused = Command::new(env!("CARGO_BIN_EXE_agent_bridge_v1"))
        .arg("--bogus")
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(2));
}
