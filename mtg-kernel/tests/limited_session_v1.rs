use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1, LimitedJsonlServerV1};
use mtg_kernel::rl_session::KernelRlJsonlServerV1;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn deck(name: &str, count: u32) -> CustomDeckV1 {
    CustomDeckV1 {
        cards: vec![CustomCardCountV1 {
            name: name.to_string(),
            count,
        }],
    }
}

fn reset(request_id: &str) -> String {
    json!({"request_type":"reset", "schema_version":1, "request_id":request_id,
        "decks":[deck("Forest",40), deck("Island",40)], "episode_id":7, "env_seed":123,
        "max_physical_decisions":4096, "max_policy_steps":8192})
    .to_string()
}

fn response(server: &mut LimitedJsonlServerV1, request: &str) -> Value {
    serde_json::from_str(&server.handle_line(request)).unwrap()
}

fn step(request_id: &str, reply: &Value) -> String {
    let decision = &reply["decision"];
    let actions = decision["legal_actions"].as_array().unwrap();
    let action = actions
        .iter()
        .find(|a| a["semantic"]["action_kind"] == "pass")
        .unwrap_or(&actions[0]);
    json!({"request_type":"step", "schema_version":reply["schema_version"], "request_id":request_id,
        "episode_id":decision["episode_id"], "expected_step":decision["step"],
        "selected_index":action["selected_index"], "selected_action_id":action["stable_id"]})
    .to_string()
}

#[test]
fn deck_resolution_preserves_order_and_accepts_large_copy_counts() {
    let mixed = CustomDeckV1 {
        cards: vec![
            CustomCardCountV1 {
                name: "Island".into(),
                count: 20,
            },
            CustomCardCountV1 {
                name: "Forest".into(),
                count: 21,
            },
        ],
    };
    let expected: Vec<u16> = [
        vec![card_id_by_name("Island").unwrap(); 20],
        vec![card_id_by_name("Forest").unwrap(); 21],
    ]
    .concat();
    assert_eq!(mixed.resolve().unwrap(), expected);
    for invalid in [
        deck("Forest", 39),
        deck("Forest", 0),
        deck("Forest", 10_001),
        deck("Unknown", 40),
    ] {
        assert!(invalid.resolve().is_err());
    }
    let token = CARD_DEFS.iter().find(|card| card.is_token).unwrap();
    assert!(deck(token.name, 40).resolve().is_err());
}

#[test]
fn reset_has_content_identity_and_retries_are_byte_identical() {
    let mut server = LimitedJsonlServerV1::new();
    let request = reset("reset");
    let first = server.handle_line(&request);
    assert_eq!(first, server.handle_line(&request));
    let reply: Value = serde_json::from_str(&first).unwrap();
    assert_eq!(reply["protocol"], "kernel_limited_jsonl");
    let ids = reply["decision"]["deck_ids"].as_array().unwrap();
    assert_ne!(ids[0], ids[1]);
    assert!(ids[0].as_str().unwrap().starts_with("custom-v1:"));
    let mut changed: Value = serde_json::from_str(&request).unwrap();
    changed["env_seed"] = json!(124);
    assert_eq!(
        response(&mut server, &changed.to_string())["error"]["code"],
        "request_id_reuse_mismatch"
    );
    let next = step("step", &reply);
    let stepped = server.handle_line(&next);
    assert_eq!(stepped, server.handle_line(&next));
}

#[test]
fn identities_follow_expanded_content_not_row_chunking() {
    let mut server = LimitedJsonlServerV1::new();
    let first = response(&mut server, &reset("a"));
    let mut chunked: Value = serde_json::from_str(&reset("b")).unwrap();
    chunked["decks"][0]["cards"] =
        json!([{"name":"Forest","count":5},{"name":"Forest","count":35}]);
    let second = response(&mut server, &chunked.to_string());
    assert_eq!(
        first["decision"]["deck_ids"],
        second["decision"]["deck_ids"]
    );
    assert_eq!(
        first["decision"]["deck_hashes"],
        second["decision"]["deck_hashes"]
    );
}

#[test]
fn invalid_second_deck_and_bad_steps_preserve_the_active_game() {
    let mut server = LimitedJsonlServerV1::new();
    let mut control = LimitedJsonlServerV1::new();
    let active = response(&mut server, &reset("a"));
    response(&mut control, &reset("a"));
    let mut bad: Value = serde_json::from_str(&reset("invalid")).unwrap();
    bad["decks"][1]["cards"][0]["name"] = json!("Unimplemented");
    let refused = response(&mut server, &bad.to_string());
    assert_eq!(refused["error"]["code"], "unsupported_deck");
    assert!(refused["error"]["message"]
        .as_str()
        .unwrap()
        .contains("seat 1"));
    let mut invalid_step: Value = serde_json::from_str(&step("bad-step", &active)).unwrap();
    invalid_step["selected_action_id"] = json!("incorrect");
    assert_eq!(
        response(&mut server, &invalid_step.to_string())["response_type"],
        "error"
    );
    let valid = step("valid", &active);
    assert_eq!(server.handle_line(&valid), control.handle_line(&valid));
}

#[test]
fn strict_decode_and_version_errors_do_not_replace_the_game() {
    let mut server = LimitedJsonlServerV1::new();
    let mut control = LimitedJsonlServerV1::new();
    let active = response(&mut server, &reset("a"));
    response(&mut control, &reset("a"));
    let duplicate = reset("duplicate").replacen(
        "\"schema_version\":1",
        "\"schema_version\":1,\"schema_version\":1",
        1,
    );
    assert_eq!(
        response(&mut server, &duplicate)["error"]["code"],
        "malformed_request"
    );
    let mut bad: Value = serde_json::from_str(&reset("wrong-version")).unwrap();
    bad["schema_version"] = json!(5);
    assert_eq!(
        response(&mut server, &bad.to_string())["error"]["code"],
        "schema_version_mismatch"
    );
    bad["schema_version"] = json!(1);
    bad["request_id"] = json!("");
    assert_eq!(
        response(&mut server, &bad.to_string())["error"]["code"],
        "malformed_request"
    );
    bad["request_id"] = json!("unknown-field");
    bad["sideboard"] = json!([]);
    assert_eq!(
        response(&mut server, &bad.to_string())["error"]["code"],
        "malformed_request"
    );
    let valid = step("valid", &active);
    assert_eq!(server.handle_line(&valid), control.handle_line(&valid));
}

#[test]
fn custom_reset_is_not_accepted_by_the_catalog_wire_protocol() {
    let mut server = KernelRlJsonlServerV1::new();
    let result: Value = serde_json::from_str(&server.handle_line(&reset("custom"))).unwrap();
    assert_eq!(result["response_type"], "error");
}

fn process_smoke(engine_priority: bool) -> (String, Value) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kernel_limited_env"));
    if engine_priority {
        command.arg("--engine-priority-v1");
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut transcript = Sha256::new();
    let mut request = reset("reset");
    if engine_priority {
        let mut value: Value = serde_json::from_str(&request).unwrap();
        value["schema_version"] = json!(2);
        request = value.to_string();
    }
    for index in 0..8192 {
        writeln!(input, "{request}").unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        assert!(output.read_line(&mut line).unwrap() > 0);
        transcript.update(request.as_bytes());
        transcript.update(b"\n");
        transcript.update(line.as_bytes());
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_ne!(reply["response_type"], "error", "{reply}");
        assert_eq!(reply["schema_version"], if engine_priority { 2 } else { 1 });
        if engine_priority {
            assert_eq!(reply["priority_mode"], "engine_windows_v1");
        } else {
            assert!(reply.get("priority_mode").is_none());
        }
        if reply["response_type"] == "terminal" {
            drop(input);
            assert!(child.wait().unwrap().success());
            return (format!("{:x}", transcript.finalize()), reply);
        }
        request = step(&format!("step-{index}"), &reply);
    }
    child.kill().unwrap();
    child.wait().unwrap();
    panic!("smoke game did not reach a terminal");
}

#[test]
fn complete_40_card_game_replays_bit_identically_through_the_binary() {
    let first = process_smoke(false);
    let second = process_smoke(false);
    assert_eq!(first, second);
    assert_eq!(first.1["terminal"]["terminal_classification"], "natural");
    assert_eq!(first.1["terminal"]["terminal_code"], "natural_game_over");
}

#[test]
fn opt_in_complete_game_replays_with_explicit_windows_and_mode_identity() {
    let first = process_smoke(true);
    let second = process_smoke(true);
    assert_eq!(first, second);
    assert_eq!(first.1["terminal"]["terminal_classification"], "natural");
    assert!(first.1["terminal"]["policy_step_count"].as_u64().unwrap() > 200);
}

#[test]
fn modes_refuse_each_others_schema_without_mutating_the_active_game() {
    for engine_priority in [false, true] {
        let mut server = if engine_priority {
            LimitedJsonlServerV1::new_with_engine_priority_v1()
        } else {
            LimitedJsonlServerV1::new()
        };
        let mut request: Value = serde_json::from_str(&reset("reset")).unwrap();
        request["schema_version"] = json!(if engine_priority { 2 } else { 1 });
        let raw = request.to_string();
        let active = server.handle_line(&raw);
        assert_eq!(active, server.handle_line(&raw));
        let reply: Value = serde_json::from_str(&active).unwrap();
        let context = &reply["decision"]["observation"]["projection"]["surface_context"];
        if engine_priority {
            assert_eq!(context["engine_priority_version"], 1);
        } else {
            assert!(context.get("engine_priority_version").is_none());
        }
        let valid_step = step("step", &reply);
        let mut invalid: Value = serde_json::from_str(&valid_step).unwrap();
        invalid["schema_version"] = json!(if engine_priority { 1 } else { 2 });
        assert_eq!(
            response(&mut server, &invalid.to_string())["error"]["code"],
            "schema_version_mismatch"
        );
        let next = server.handle_line(&valid_step);
        assert_eq!(next, server.handle_line(&valid_step));
        assert_ne!(
            serde_json::from_str::<Value>(&next).unwrap()["response_type"],
            "error"
        );
    }
}
