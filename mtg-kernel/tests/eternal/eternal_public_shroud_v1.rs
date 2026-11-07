//! Limited schema 5: the public shroud contract. Its policy observation
//! reports a permanent's shroud (Argothian Enchantress) and a player's
//! (Solitary Confinement), where schema 4 refuses the game and keeps its
//! historical bytes.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::CARD_DEFS;
use mtg_kernel::limited_session_v1::LimitedJsonlServerV1;
use serde_json::{json, Value};

const MAX_STEPS: usize = 4096;

fn deck(land: &str, card: &str) -> Value {
    json!({"cards": [{"name": land, "count": 17}, {"name": card, "count": 23}]})
}

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

/// The battlefield cards named `name` in a decision's public projection.
fn battlefield_named<'a>(decision: &'a Value, name: &str) -> Vec<&'a Value> {
    decision["observation"]["projection"]["battlefield"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|cards| cards.as_array().into_iter().flatten())
        .filter(|card| {
            card["stable"]["card_db_id"]
                .as_u64()
                .is_some_and(|id| CARD_DEFS[id as usize].name == name)
        })
        .collect()
}

/// What a game showed before it ended or reached a decision with `card` on
/// the battlefield.
struct Outcome {
    /// The first decision with `card` on the battlefield.
    decision: Option<Value>,
    /// The terminal reason, if the game ended first.
    terminal_reason: Option<String>,
    /// Every reply line, in order.
    replies: Vec<String>,
}

/// Plays both seats with the same ranked preference (keep, land, spell) until
/// a decision shows `card` on the battlefield or the game ends.
fn play(schema: u32, land: &str, card: &str) -> Outcome {
    let mut server = match schema {
        4 => LimitedJsonlServerV1::new_with_london_mulligans_v1(),
        5 => LimitedJsonlServerV1::new_with_public_shroud_v1(),
        _ => unreachable!(),
    };
    let mut request = json!({"schema_version": schema, "request_type": "reset",
        "request_id": "reset", "decks": [deck(land, card), deck(land, card)],
        "episode_id": 7, "env_seed": 0x5348_524f_5544_u64,
        "max_physical_decisions": 65536, "max_policy_steps": MAX_STEPS})
    .to_string();
    let mut replies = Vec::new();
    for index in 0..MAX_STEPS {
        let raw = server.handle_line(&request);
        let reply: Value = serde_json::from_str(&raw).unwrap();
        replies.push(raw);
        assert_ne!(reply["response_type"], "error", "step {index}: {reply}");
        if reply["response_type"] == "terminal" {
            return Outcome {
                decision: None,
                terminal_reason: Some(
                    reply["terminal"]["terminal_reason"]
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ),
                replies,
            };
        }
        let decision = &reply["decision"];
        if !battlefield_named(decision, card).is_empty() {
            return Outcome {
                decision: Some(decision.clone()),
                terminal_reason: None,
                replies,
            };
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
    panic!("{card} never reached the battlefield");
}

#[test]
fn schema_5_reports_a_permanents_shroud_that_schema_4_refuses() {
    let public = play(5, "Forest", "Argothian Enchantress");
    assert!(public
        .replies
        .iter()
        .all(|reply| reply.contains(r#""observation_rules":"public_shroud_v1""#)));
    let decision = public.decision.expect("schema 5 admits shroud");
    // Exactly the Enchantresses have shroud; no seat does.
    let shroud = &decision["observation"]["projection"]["public_shroud"];
    let enchantresses: Vec<&Value> = battlefield_named(&decision, "Argothian Enchantress")
        .into_iter()
        .map(|card| &card["stable"])
        .collect();
    assert!(!enchantresses.is_empty());
    assert_eq!(
        shroud["permanents"]
            .as_array()
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        enchantresses
    );
    assert_eq!(shroud["players"], json!([false, false]));

    let legacy = play(4, "Forest", "Argothian Enchantress");
    assert_eq!(
        legacy.terminal_reason.as_deref(),
        Some("fail_closed:observation:shroud requires an explicit public keyword representation")
    );
    // Every reply before the refusal keeps the historical fields.
    let (_terminal, decisions) = legacy.replies.split_last().unwrap();
    for reply in decisions {
        assert!(!reply.contains("shroud"), "{reply}");
        assert!(!reply.contains("observation_rules"), "{reply}");
    }
    // The two schemas play the same game: schema 4 halts exactly where
    // schema 5 first reports the Enchantress.
    assert_eq!(public.replies.len(), legacy.replies.len());
}

#[test]
fn schema_5_reports_a_players_shroud_that_schema_4_refuses() {
    let public = play(5, "Plains", "Solitary Confinement");
    let decision = public.decision.expect("schema 5 admits player shroud");
    let controller =
        &battlefield_named(&decision, "Solitary Confinement")[0]["stable"]["controller"];
    let seat = usize::from(controller == "p1");
    let mut expected = [false, false];
    expected[seat] = true;
    let shroud = &decision["observation"]["projection"]["public_shroud"];
    assert_eq!(shroud["players"], json!(expected));
    assert_eq!(shroud["permanents"], json!([]));

    let legacy = play(4, "Plains", "Solitary Confinement");
    assert_eq!(
        legacy.terminal_reason.as_deref(),
        Some("fail_closed:observation:player shroud requires an explicit public representation")
    );
    let (_terminal, decisions) = legacy.replies.split_last().unwrap();
    for reply in decisions {
        assert!(!reply.contains("shroud"), "{reply}");
    }
    assert_eq!(public.replies.len(), legacy.replies.len());
}
