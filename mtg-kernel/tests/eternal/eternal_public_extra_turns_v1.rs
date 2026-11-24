//! Limited schema 5 public contract: extra turns. Once an extra turn is
//! scheduled (Time Walk, Time Vault's ability, Emrakul, the Aeons Torn's cast
//! trigger) the policy observation reports the public turn schedule
//! (`projection.turn_schedule`) and the session follows the game, where
//! schema 4 refuses the game and keeps its historical bytes.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::limited_session_v1::LimitedJsonlServerV1;
use serde_json::{json, Value};

const MAX_STEPS: usize = 32_768;

fn deck(rows: &[(&str, u32)]) -> Value {
    json!({"cards": rows
        .iter()
        .map(|(name, count)| json!({"name": name, "count": count}))
        .collect::<Vec<_>>()})
}

/// Keep, play a land, cast a spell, and otherwise pass.
fn rank(action: &Value) -> u8 {
    let semantic = &action["semantic"];
    match semantic["action_kind"].as_str().unwrap() {
        "choose_london_mulligan" if semantic["mulligan"] == false => 0,
        "play_land" => 0,
        "cast_spell" => 1,
        "pass" => 10,
        "activate_mana_ability" | "activate_ability" => 11,
        _ => 5,
    }
}

struct Game {
    /// Every decision reply, in order.
    decisions: Vec<Value>,
    terminal: Value,
    /// Every reply line, in order.
    replies: Vec<String>,
}

/// Plays both seats with the same ranked preference until the game ends.
fn play(schema: u32, decks: [Value; 2]) -> Game {
    let mut server = match schema {
        4 => LimitedJsonlServerV1::new_with_london_mulligans_v1(),
        5 => LimitedJsonlServerV1::new_with_public_shroud_v1(),
        _ => unreachable!(),
    };
    let mut request = json!({"schema_version": schema, "request_type": "reset",
        "request_id": "reset", "decks": decks,
        "episode_id": 11, "env_seed": 0x5449_4d45_u64,
        "max_physical_decisions": 65536, "max_policy_steps": MAX_STEPS})
    .to_string();
    let mut decisions = Vec::new();
    let mut replies = Vec::new();
    for index in 0..MAX_STEPS {
        let raw = server.handle_line(&request);
        let reply: Value = serde_json::from_str(&raw).unwrap();
        replies.push(raw);
        assert_ne!(reply["response_type"], "error", "step {index}: {reply}");
        if reply["response_type"] == "terminal" {
            return Game {
                decisions,
                terminal: reply["terminal"].clone(),
                replies,
            };
        }
        let decision = reply["decision"].clone();
        let action = decision["legal_actions"]
            .as_array()
            .unwrap()
            .iter()
            .min_by_key(|action| rank(action))
            .unwrap();
        request = json!({"schema_version": schema, "request_type": "step",
            "request_id": format!("step-{index}"), "episode_id": decision["episode_id"],
            "expected_step": decision["step"], "selected_index": action["selected_index"],
            "selected_action_id": action["stable_id"]})
        .to_string();
        decisions.push(decision);
    }
    panic!("the game did not end within {MAX_STEPS} steps");
}

fn projection(decision: &Value) -> &Value {
    &decision["observation"]["projection"]
}

/// The (round, active seat) of each turn in the order the session showed
/// them: one entry per maximal run of decisions with the same pair and the
/// same public occurrence.
fn turns(decisions: &[Value]) -> Vec<(u64, String, Option<u64>)> {
    let mut seen: Vec<(u64, String, Option<u64>)> = Vec::new();
    for decision in decisions {
        let projection = projection(decision);
        let Some(round) = projection["turn"].as_u64() else {
            continue;
        };
        if round == 0 {
            continue;
        }
        let row = (
            round,
            projection["active_player"].as_str().unwrap().to_owned(),
            projection["turn_schedule"]["occurrence"].as_u64(),
        );
        if seen.last() != Some(&row) {
            seen.push(row);
        }
    }
    seen
}

#[test]
fn schema_5_follows_time_walk_extra_turns_that_schema_4_refuses() {
    let decks = || {
        [
            deck(&[("Island", 17), ("Time Walk", 23)]),
            deck(&[("Island", 40)]),
        ]
    };
    let public = play(5, decks());
    assert_eq!(
        public.terminal["terminal_code"], "natural_game_over",
        "{}",
        public.terminal
    );

    // Before the first Time Walk resolves the schedule is absent, so those
    // replies keep the bytes they had before this contract.
    let first = public
        .decisions
        .iter()
        .position(|decision| !projection(decision)["turn_schedule"].is_null())
        .expect("a Time Walk resolved");
    for reply in &public.replies[..first] {
        assert!(!reply.contains("turn_schedule"), "{reply}");
    }

    // The first schedule: Time Walk's controller (seat 0) has one extra turn
    // queued after this one, and the regular order resumes with seat 1.
    let schedule = &projection(&public.decisions[first])["turn_schedule"];
    assert_eq!(schedule["current_extra"], Value::Null, "{schedule}");
    assert_eq!(schedule["next_regular_player"], "p1", "{schedule}");
    let queued = schedule["queued_next_first"].as_array().unwrap();
    assert_eq!(queued.len(), 1, "{schedule}");
    assert_eq!(queued[0]["captured_controller"], "p0", "{schedule}");
    assert_eq!(queued[0]["physical_owner"], "p0", "{schedule}");
    assert_eq!(queued[0]["spell_copy"], false, "{schedule}");
    assert_eq!(projection(&public.decisions[first])["active_player"], "p0");

    // Seat 0 takes at least one extra turn: two consecutive turns with the
    // same round and active seat, told apart only by the public occurrence,
    // and during the second the schedule names the extra turn being taken.
    let turns = turns(&public.decisions);
    assert!(
        turns.windows(2).any(|pair| {
            pair[0].0 == pair[1].0
                && pair[0].1 == "p0"
                && pair[1].1 == "p0"
                && pair[1].2 == pair[0].2.map(|occurrence| occurrence + 1)
        }),
        "{turns:?}"
    );
    assert!(public.decisions.iter().any(|decision| {
        let projection = projection(decision);
        projection["active_player"] == "p0"
            && projection["turn_schedule"]["current_extra"]["captured_controller"] == "p0"
    }));
    // Every occurrence is one more than the last: no turn is skipped.
    let occurrences: Vec<u64> = turns.iter().filter_map(|turn| turn.2).collect();
    assert!(
        occurrences.windows(2).all(|pair| pair[1] == pair[0] + 1),
        "{occurrences:?}"
    );

    // Schema 4 keeps its refusal at exactly the decision where schema 5 first
    // reports the schedule, and every reply before it is unchanged.
    let legacy = play(4, decks());
    assert_eq!(
        legacy.terminal["terminal_reason"],
        "fail_closed:observation:extra turns require their selected public/Human successor"
    );
    assert_eq!(legacy.decisions.len(), first);
    for reply in &legacy.replies {
        assert!(!reply.contains("turn_schedule"), "{reply}");
    }
}

#[test]
fn schema_5_follows_emrakul_cast_trigger_extra_turn() {
    // Emrakul's extra turn comes from its cast trigger, so it is scheduled
    // while Emrakul itself is still on the stack.
    let decks = [
        deck(&[("Island", 28), ("Emrakul, the Aeons Torn", 12)]),
        deck(&[("Island", 40)]),
    ];
    let public = play(5, decks);
    assert_eq!(
        public.terminal["terminal_code"], "natural_game_over",
        "{}",
        public.terminal
    );
    let scheduled = public
        .decisions
        .iter()
        .find(|decision| !projection(decision)["turn_schedule"].is_null())
        .expect("Emrakul was cast");
    let schedule = &projection(scheduled)["turn_schedule"];
    let queued = schedule["queued_next_first"].as_array().unwrap();
    assert_eq!(queued.len(), 1, "{schedule}");
    assert_eq!(queued[0]["captured_controller"], "p0", "{schedule}");
    assert!(public.decisions.iter().any(|decision| {
        projection(decision)["turn_schedule"]["current_extra"]["captured_controller"] == "p0"
    }));
}
