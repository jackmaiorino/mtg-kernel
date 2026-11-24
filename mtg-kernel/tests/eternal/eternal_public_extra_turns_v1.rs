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

/// What one decision showed of the turn structure.
#[derive(Debug, Clone, PartialEq)]
struct Shown {
    round: u64,
    active: String,
    /// `projection.turn_schedule`, or null while it is absent.
    schedule: Value,
    stack_items: usize,
}

struct Game {
    /// One row per decision reply, in order.
    shown: Vec<Shown>,
    /// The terminal reply, unless `stop` ended the script first.
    terminal: Option<Value>,
    /// Whether any reply before the first reported schedule mentions it.
    early_mention: bool,
}

/// Plays both seats with the same ranked preference until the game ends or
/// `stop` accepts a decision.
fn play(schema: u32, decks: [Value; 2], stop: impl Fn(&Shown) -> bool) -> Game {
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
    let mut game = Game {
        shown: Vec::new(),
        terminal: None,
        early_mention: false,
    };
    let mut reported = false;
    for index in 0..MAX_STEPS {
        let raw = server.handle_line(&request);
        let reply: Value = serde_json::from_str(&raw).unwrap();
        assert_ne!(reply["response_type"], "error", "step {index}: {reply}");
        if reply["response_type"] == "terminal" {
            game.terminal = Some(reply["terminal"].clone());
            return game;
        }
        let decision = &reply["decision"];
        let projection = &decision["observation"]["projection"];
        let shown = Shown {
            round: projection["turn"].as_u64().unwrap(),
            active: projection["active_player"].as_str().unwrap().to_owned(),
            schedule: projection["turn_schedule"].clone(),
            stack_items: projection["stack"].as_array().unwrap().len(),
        };
        reported |= !shown.schedule.is_null();
        game.early_mention |= !reported && raw.contains("turn_schedule");
        let done = stop(&shown);
        game.shown.push(shown);
        if done {
            return game;
        }
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
    }
    panic!("the script did not end within {MAX_STEPS} steps");
}

/// Each turn the session showed, in order: one entry per maximal run of
/// decisions with the same round, active seat and public occurrence.
fn turns(shown: &[Shown]) -> Vec<(u64, String, Option<u64>)> {
    let mut seen: Vec<(u64, String, Option<u64>)> = Vec::new();
    for row in shown.iter().filter(|row| row.round != 0) {
        let turn = (
            row.round,
            row.active.clone(),
            row.schedule["occurrence"].as_u64(),
        );
        if seen.last() != Some(&turn) {
            seen.push(turn);
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
    // Seat 0 keeps taking extra turns and draws its library out.
    let public = play(5, decks(), |_| false);
    let terminal = public.terminal.as_ref().unwrap();
    assert_eq!(terminal["terminal_code"], "natural_game_over", "{terminal}");

    // Before the first Time Walk resolves the schedule is absent, so those
    // replies keep the bytes they had before this contract.
    assert!(!public.early_mention);
    let first = public
        .shown
        .iter()
        .position(|row| !row.schedule.is_null())
        .expect("a Time Walk resolved");

    // The first schedule: Time Walk's controller (seat 0) has one extra turn
    // queued after this one, and the regular order resumes with seat 1.
    let row = &public.shown[first];
    let schedule = &row.schedule;
    assert_eq!(row.active, "p0");
    assert_eq!(schedule["occurrence"], row.round, "{schedule}");
    assert_eq!(schedule["legacy_round"], row.round, "{schedule}");
    assert_eq!(schedule["current_extra"], Value::Null, "{schedule}");
    assert_eq!(schedule["previous_player"], "p1", "{schedule}");
    assert_eq!(schedule["next_regular_player"], "p1", "{schedule}");
    assert_eq!(
        schedule["queued_next_first"],
        json!([{"creation_ordinal": 0, "captured_controller": "p0",
            "physical_owner": "p0", "spell_copy": false}]),
        "{schedule}"
    );

    // Seat 0 then takes that extra turn: the next turn has the same round and
    // active seat, the next occurrence, and names the extra turn being taken.
    let turns = turns(&public.shown);
    let scheduled_turn = turns
        .iter()
        .position(|turn| turn.2 == schedule["occurrence"].as_u64())
        .unwrap();
    assert_eq!(
        turns[scheduled_turn + 1],
        (row.round, "p0".to_owned(), Some(row.round + 1)),
        "{turns:?}"
    );
    let extra = public
        .shown
        .iter()
        .find(|shown| shown.schedule["occurrence"] == row.round + 1)
        .unwrap();
    assert_eq!(
        extra.schedule["current_extra"]["captured_controller"], "p0",
        "{}",
        extra.schedule
    );
    assert_eq!(extra.schedule["previous_player"], "p0");
    // Every occurrence is one more than the last: no turn is skipped, and
    // the round only advances on seat 0's regular turns.
    let occurrences: Vec<u64> = turns.iter().filter_map(|turn| turn.2).collect();
    assert!(
        occurrences.windows(2).all(|pair| pair[1] == pair[0] + 1),
        "{occurrences:?}"
    );
    assert!(turns.windows(2).all(|pair| pair[1].0 >= pair[0].0));

    // Schema 4 keeps its refusal at exactly the decision where schema 5 first
    // reports the schedule, and never mentions it.
    let legacy = play(4, decks(), |_| false);
    assert_eq!(
        legacy.terminal.unwrap()["terminal_reason"],
        "fail_closed:observation:extra turns require their selected public/Human successor"
    );
    assert_eq!(legacy.shown.len(), first);
    assert!(!legacy.early_mention);
    assert_eq!(legacy.shown[..], public.shown[..first]);
}

#[test]
fn schema_5_follows_emrakul_cast_trigger_extra_turn() {
    // Emrakul's extra turn comes from its cast trigger, so it is scheduled
    // while Emrakul itself is still on the stack. The script stops once the
    // extra turn is being taken: this seat could cast an Emrakul every turn.
    let decks = || {
        [
            deck(&[("Island", 28), ("Emrakul, the Aeons Torn", 12)]),
            deck(&[("Island", 40)]),
        ]
    };
    let taking_extra_turn = |shown: &Shown| !shown.schedule["current_extra"].is_null();
    let public = play(5, decks(), taking_extra_turn);
    assert!(public.terminal.is_none(), "{:?}", public.terminal);
    assert!(!public.early_mention);
    let first = public
        .shown
        .iter()
        .position(|row| !row.schedule.is_null())
        .expect("Emrakul was cast");
    let row = &public.shown[first];
    assert_eq!(row.active, "p0");
    assert!(row.stack_items >= 1, "Emrakul is still on the stack");
    assert_eq!(
        row.schedule["queued_next_first"],
        json!([{"creation_ordinal": 0, "captured_controller": "p0",
            "physical_owner": "p0", "spell_copy": false}]),
        "{}",
        row.schedule
    );
    let last = public.shown.last().unwrap();
    assert_eq!(last.active, "p0");
    assert_eq!(last.round, row.round);
    assert_eq!(
        last.schedule["current_extra"]["captured_controller"], "p0",
        "{}",
        last.schedule
    );

    let legacy = play(4, decks(), taking_extra_turn);
    assert_eq!(
        legacy.terminal.unwrap()["terminal_reason"],
        "fail_closed:observation:extra turns require their selected public/Human successor"
    );
    assert_eq!(legacy.shown.len(), first);
}
