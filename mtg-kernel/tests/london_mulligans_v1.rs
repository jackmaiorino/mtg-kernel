use mtg_kernel::card_def::card_id_by_name;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::LimitedJsonlServerV1;
use mtg_kernel::london_mulligan_v1::enable_london_mulligans_v1;
use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
use mtg_kernel::rl::{
    build_deck_pair_state_with_starting_player_v1, observe_policy_v5, observe_policy_v6,
};
use mtg_kernel::state::{GameState, Step, Zone};
use serde_json::{json, Value};
#[cfg(feature = "limited-fdn-fixtures")]
use sha2::{Digest, Sha256};
#[cfg(feature = "limited-fdn-fixtures")]
use std::io::{BufRead, BufReader, Write};
#[cfg(feature = "limited-fdn-fixtures")]
use std::process::{Command, Stdio};

fn opening(starting: PlayerId) -> GameState {
    let cards = [
        vec![card_id_by_name("Forest").unwrap(); 20],
        vec![card_id_by_name("Island").unwrap(); 20],
    ]
    .concat();
    let mut state =
        build_deck_pair_state_with_starting_player_v1(123, &cards, &cards, starting).unwrap();
    enable_london_mulligans_v1(&mut state).unwrap();
    state
}

fn announce(state: &mut GameState, player: PlayerId, count: u8, mulligan: bool) {
    assert_eq!(
        engine::advance_until_decision(state),
        Decision::ChooseLondonMulligan {
            player,
            mulligan_count: count,
        }
    );
    assert_eq!(state.step, Step::Untap);
    engine::step(state, Action::ChooseLondonMulligan { mulligan }).unwrap();
}

fn bottom(state: &mut GameState, player: PlayerId, remaining: u8) -> ObjectId {
    let Decision::ChooseLondonBottom {
        player: actor,
        remaining: actual,
        candidates,
    } = engine::advance_until_decision(state)
    else {
        panic!("expected bottom choice")
    };
    assert_eq!((actor, actual), (player, remaining));
    let card = candidates[0];
    engine::step(state, Action::ChooseLondonBottom(card)).unwrap();
    card
}

#[test]
fn policy_v5_and_v6_preserve_london_public_state_and_own_hand() {
    fn assert_observations(state: &GameState, expected: Option<Value>) {
        let surface = PolicySurfaceV5::new_with_engine_priority_v1();
        for observer in [PlayerId::P0, PlayerId::P1] {
            let v5 = observe_policy_v5(state, &surface, observer, 0, 0, 0, 1).unwrap();
            let v6 = observe_policy_v6(state, &surface, observer, 0, 0, 0, 1).unwrap();
            assert_eq!(v5.projection, v6.projection);
            assert_eq!(v5.own_hand, v6.own_hand);
            assert_eq!(
                v6.own_hand
                    .iter()
                    .map(|card| ObjectId(card.stable.arena_id))
                    .collect::<Vec<_>>(),
                state.players[observer.index()].hand
            );
            assert!(v6
                .own_hand
                .iter()
                .all(|card| card.stable.owner == observer.into()));
            let projection = serde_json::to_value(&v6.projection).unwrap();
            assert_eq!(projection.get("london_mulligans"), expected.as_ref());
        }
    }

    let cards = vec![card_id_by_name("Forest").unwrap(); 40];
    let legacy =
        build_deck_pair_state_with_starting_player_v1(123, &cards, &cards, PlayerId::P0).unwrap();
    assert_observations(&legacy, None);

    let mut state = opening(PlayerId::P0);
    assert_observations(
        &state,
        Some(json!({"phase":"announce", "counts":[0,0], "kept":[false,false]})),
    );
    announce(&mut state, PlayerId::P0, 0, true);
    announce(&mut state, PlayerId::P1, 0, false);
    engine::advance_until_decision(&mut state);
    assert_observations(
        &state,
        Some(json!({"phase":"bottom", "counts":[1,0], "kept":[false,true]})),
    );
    bottom(&mut state, PlayerId::P0, 1);
    engine::advance_until_decision(&mut state);
    assert_observations(
        &state,
        Some(json!({"phase":"announce", "counts":[1,0], "kept":[false,true]})),
    );
    announce(&mut state, PlayerId::P0, 1, false);
    engine::advance_until_decision(&mut state);
    assert_observations(
        &state,
        Some(json!({"phase":"complete", "counts":[1,0], "kept":[true,true]})),
    );
}

#[test]
fn keep_both_preserves_opening_hands_and_first_turn_draw_skip() {
    for starting in [PlayerId::P0, PlayerId::P1] {
        let mut state = opening(starting);
        let hands = state.players.clone().map(|player| player.hand);
        let libraries = state.players.clone().map(|player| player.library);
        announce(&mut state, starting, 0, false);
        announce(&mut state, starting.opponent(), 0, false);
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == starting)
        );
        assert!(state.london_mulligans_v1.as_ref().unwrap().is_complete());
        assert_eq!(state.players.clone().map(|player| player.hand), hands);
        assert_eq!(
            state.players.clone().map(|player| player.library),
            libraries
        );
        // Skipping the first draw preserves the draw-step priority window.
        for next in [Step::Draw, Step::Main1] {
            engine::step(&mut state, Action::Pass).unwrap();
            engine::advance_until_decision(&mut state);
            engine::step(&mut state, Action::Pass).unwrap();
            engine::advance_until_decision(&mut state);
            assert_eq!(state.step, next);
            assert_eq!(state.players[starting.index()].hand.len(), 7);
        }
    }
}

#[test]
fn announcements_complete_before_redraw_and_both_redraw_before_bottoming() {
    let mut state = opening(PlayerId::P0);
    let hands = state.players.clone().map(|player| player.hand);
    announce(&mut state, PlayerId::P0, 0, true);
    assert_eq!(state.players.clone().map(|player| player.hand), hands);
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseLondonMulligan {
            player: PlayerId::P1,
            ..
        }
    ));
    assert_eq!(state.players.clone().map(|player| player.hand), hands);
    engine::step(&mut state, Action::ChooseLondonMulligan { mulligan: true }).unwrap();
    let decision = engine::advance_until_decision(&mut state);
    assert!(matches!(
        decision,
        Decision::ChooseLondonBottom {
            player: PlayerId::P0,
            remaining: 1,
            ..
        }
    ));
    assert_eq!(state.london_mulligans_v1.as_ref().unwrap().counts(), [1, 1]);
    assert!(state.players.iter().all(|player| player.hand.len() == 7));
    assert_ne!(state.players[0].hand, hands[0]);
    assert_ne!(state.players[1].hand, hands[1]);
    bottom(&mut state, PlayerId::P0, 1);
    bottom(&mut state, PlayerId::P1, 1);
    announce(&mut state, PlayerId::P0, 1, false);
    announce(&mut state, PlayerId::P1, 1, false);
    engine::advance_until_decision(&mut state);
    assert!(state.players.iter().all(|player| player.hand.len() == 6));
}

#[test]
fn repeated_mulligans_bottom_exact_count_before_next_announcement() {
    let mut state = opening(PlayerId::P0);
    announce(&mut state, PlayerId::P0, 0, true);
    announce(&mut state, PlayerId::P1, 0, false);
    bottom(&mut state, PlayerId::P0, 1);
    assert_eq!(state.players[0].hand.len(), 6);
    announce(&mut state, PlayerId::P0, 1, true);
    // Kept P1 skips this and all subsequent rounds.
    bottom(&mut state, PlayerId::P0, 2);
    bottom(&mut state, PlayerId::P0, 1);
    assert_eq!(state.players[0].hand.len(), 5);
    announce(&mut state, PlayerId::P0, 2, false);
    engine::advance_until_decision(&mut state);
    assert_eq!(state.players[0].hand.len(), 5);
    assert_eq!(state.players[1].hand.len(), 7);
    assert_eq!(
        state.london_mulligans_v1.as_ref().unwrap().kept(),
        [true, true]
    );
}

#[test]
fn zero_card_keep_is_forced_after_seven_mulligans() {
    let mut state = opening(PlayerId::P1);
    let player = PlayerId::P1;
    announce(&mut state, player, 0, true);
    announce(&mut state, player.opponent(), 0, false);
    for count in 1..=7 {
        for remaining in (1..=count).rev() {
            bottom(&mut state, player, remaining);
        }
        assert_eq!(
            state.players[player.index()].hand.len(),
            7 - usize::from(count)
        );
        if count < 7 {
            announce(&mut state, player, count, true);
        }
    }
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.london_mulligans_v1.as_ref().unwrap().counts(), [0, 7]);
    assert!(state.london_mulligans_v1.as_ref().unwrap().is_complete());
    let before = state.clone();
    assert!(engine::step(&mut state, Action::ChooseLondonMulligan { mulligan: true }).is_err());
    assert_eq!(state, before);
}

#[test]
fn bottom_selection_order_is_exact_and_private() {
    let mut state = opening(PlayerId::P0);
    announce(&mut state, PlayerId::P0, 0, true);
    announce(&mut state, PlayerId::P1, 0, false);
    bottom(&mut state, PlayerId::P0, 1);
    announce(&mut state, PlayerId::P0, 1, true);
    let first = bottom(&mut state, PlayerId::P0, 2);
    let second = bottom(&mut state, PlayerId::P0, 1);
    assert_eq!(
        &state.players[0].library[state.players[0].library.len() - 2..],
        &[first, second]
    );
    let own = state.known_library_cards(PlayerId::P0, PlayerId::P0);
    assert_eq!(
        own.iter().map(|entry| entry.object).collect::<Vec<_>>(),
        [first, second]
    );
    assert!(state
        .known_library_cards(PlayerId::P1, PlayerId::P0)
        .is_empty());
}

#[test]
fn invalid_bottom_choice_is_atomic_for_foreign_duplicate_and_out_of_zone_cards() {
    let mut state = opening(PlayerId::P0);
    announce(&mut state, PlayerId::P0, 0, true);
    announce(&mut state, PlayerId::P1, 0, false);
    engine::advance_until_decision(&mut state);
    for card in [
        state.players[1].hand[0],
        state.players[0].library[0],
        ObjectId(u32::MAX),
    ] {
        let before = state.clone();
        assert!(engine::step(&mut state, Action::ChooseLondonBottom(card)).is_err());
        assert_eq!(state, before);
    }
    let first = bottom(&mut state, PlayerId::P0, 1);
    announce(&mut state, PlayerId::P0, 1, true);
    let chosen = bottom(&mut state, PlayerId::P0, 2);
    let before = state.clone();
    assert!(engine::step(&mut state, Action::ChooseLondonBottom(chosen)).is_err());
    assert_eq!(state, before);
    // The earlier physical card is legal again only if the redraw actually
    // returned its new incarnation to hand.
    if !state.players[0].hand.contains(&first) {
        assert!(engine::step(&mut state, Action::ChooseLondonBottom(first)).is_err());
        assert_eq!(state, before);
    }
}

#[test]
fn stale_hand_incarnation_is_rejected_before_mutation() {
    let mut state = opening(PlayerId::P0);
    announce(&mut state, PlayerId::P0, 0, true);
    announce(&mut state, PlayerId::P1, 0, false);
    let Decision::ChooseLondonBottom { candidates, .. } =
        engine::advance_until_decision(&mut state)
    else {
        panic!("bottom")
    };
    let card = candidates[0];
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(card, Zone::Library));
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(card, Zone::Hand));
    let before = state.clone();
    assert!(engine::step(&mut state, Action::ChooseLondonBottom(card)).is_err());
    assert_eq!(state, before);
}

#[test]
fn restore_each_pregame_phase_reproduces_choices_rng_and_zone_incarnations() {
    let mut state = opening(PlayerId::P0);
    for _ in 0..20 {
        let decision = engine::advance_until_decision(&mut state);
        if state.london_mulligans_v1.as_ref().unwrap().is_complete() {
            break;
        }
        let action = match &decision {
            Decision::ChooseLondonMulligan {
                player,
                mulligan_count,
            } => Action::ChooseLondonMulligan {
                mulligan: *player == PlayerId::P0 && *mulligan_count < 2,
            },
            Decision::ChooseLondonBottom { candidates, .. } => {
                Action::ChooseLondonBottom(candidates[0])
            }
            _ => panic!("unexpected decision"),
        };
        let snapshot = state.snapshot();
        let json = serde_json::to_string(&state).unwrap();
        let mut restored: GameState = serde_json::from_str(&json).unwrap();
        assert_eq!(engine::advance_until_decision(&mut restored), decision);
        engine::step(&mut state, action.clone()).unwrap();
        let next = engine::advance_until_decision(&mut state);
        engine::step(&mut restored, action.clone()).unwrap();
        assert_eq!(engine::advance_until_decision(&mut restored), next);
        assert_eq!(restored, state);
        let after = state.clone();
        state.restore(&snapshot);
        engine::step(&mut state, action).unwrap();
        assert_eq!(engine::advance_until_decision(&mut state), next);
        assert_eq!(state, after);
    }
    assert!(state.london_mulligans_v1.as_ref().unwrap().is_complete());
}

fn reset(schema: u32, request_id: &str) -> String {
    json!({"request_type":"reset", "schema_version":schema, "request_id":request_id,
        "decks":[{"cards":[{"name":"Forest","count":40}]}, {"cards":[{"name":"Island","count":40}]}],
        "episode_id":7, "env_seed":123, "max_physical_decisions":4096, "max_policy_steps":8192}).to_string()
}

fn request_step(reply: &Value, request_id: &str, action_index: usize) -> String {
    let decision = &reply["decision"];
    let action = &decision["legal_actions"][action_index];
    json!({"request_type":"step", "schema_version":reply["schema_version"], "request_id":request_id,
        "episode_id":decision["episode_id"], "expected_step":decision["step"],
        "selected_index":action["selected_index"], "selected_action_id":action["stable_id"]})
    .to_string()
}

#[test]
fn schema_four_exposes_private_hand_and_bound_mulligan_bottom_actions() {
    let mut server = LimitedJsonlServerV1::new_with_london_mulligans_v1();
    let initial = server.handle_line(&reset(4, "reset"));
    assert_eq!(initial, server.handle_line(&reset(4, "reset")));
    let first: Value = serde_json::from_str(&initial).unwrap();
    assert_eq!(first["mulligan_rules"], "london_v1");
    assert_eq!(first["combat_rules"], "foundations_v1");
    assert_eq!(
        first["decision"]["legal_actions"][1]["semantic"]["mulligan"],
        true
    );
    let second: Value =
        serde_json::from_str(&server.handle_line(&request_step(&first, "p0-mulligan", 1))).unwrap();
    assert_eq!(
        second["decision"]["observation"]["own_hand"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
    let bottom: Value =
        serde_json::from_str(&server.handle_line(&request_step(&second, "p1-keep", 0))).unwrap();
    let observation = &bottom["decision"]["observation"];
    assert_eq!(
        observation["projection"]["london_mulligans"]["counts"],
        json!([1, 0])
    );
    assert_eq!(
        observation["projection"]["london_mulligans"]["kept"],
        json!([false, true])
    );
    assert_eq!(
        bottom["decision"]["legal_actions"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
    assert_eq!(
        bottom["decision"]["legal_actions"][0]["semantic"]["action_kind"],
        "choose_london_bottom"
    );
    assert!(observation["known_hand_cards"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row.as_array().unwrap().is_empty()));
    let valid = request_step(&bottom, "bottom", 0);
    let mut stale: Value = serde_json::from_str(&request_step(&first, "stale", 1)).unwrap();
    stale["expected_step"] = bottom["decision"]["step"].clone();
    let rejected: Value = serde_json::from_str(&server.handle_line(&stale.to_string())).unwrap();
    assert_eq!(rejected["response_type"], "error");
    let accepted = server.handle_line(&valid);
    assert_eq!(accepted, server.handle_line(&valid));
    let next: Value = serde_json::from_str(&accepted).unwrap();
    assert_eq!(
        next["decision"]["observation"]["own_hand"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert_eq!(
        next["decision"]["legal_actions"][0]["semantic"]["action_kind"],
        "choose_london_mulligan"
    );
}

#[test]
fn schemas_one_to_three_keep_historical_opening_deal_and_refuse_schema_four() {
    for (schema, mut server) in [
        (1, LimitedJsonlServerV1::new()),
        (2, LimitedJsonlServerV1::new_with_engine_priority_v1()),
        (3, LimitedJsonlServerV1::new_with_foundations_combat_v1()),
    ] {
        let old: Value = serde_json::from_str(&server.handle_line(&reset(schema, "old"))).unwrap();
        assert!(old.get("mulligan_rules").is_none());
        assert!(old["decision"]["observation"]["projection"]
            .get("london_mulligans")
            .is_none());
        assert_eq!(
            old["decision"]["observation"]["own_hand"]
                .as_array()
                .unwrap()
                .len(),
            7
        );
        let rejected: Value = serde_json::from_str(&server.handle_line(&reset(4, "new"))).unwrap();
        assert_eq!(rejected["error"]["code"], "schema_version_mismatch");
        let next: Value =
            serde_json::from_str(&server.handle_line(&request_step(&old, "step", 0))).unwrap();
        assert_ne!(next["response_type"], "error");
    }
}

#[cfg(feature = "limited-fdn-fixtures")]
fn original_fixture(source: &str) -> Value {
    let cards: Vec<Value> = source
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with("NAME:") {
                return None;
            }
            let (count, rest) = line.split_once(' ').unwrap();
            let (_, name) = rest.split_once("] ").unwrap();
            Some(json!({"name":name, "count":count.parse::<u32>().unwrap()}))
        })
        .collect();
    assert_eq!(
        cards
            .iter()
            .map(|card| card["count"].as_u64().unwrap())
            .sum::<u64>(),
        40
    );
    json!({"cards":cards})
}

#[cfg(feature = "limited-fdn-fixtures")]
fn external_fixture_game(swapped: bool, seed: u64) -> (String, Value) {
    let ug_source = include_str!("../../data/limited/fdn_v1/FDN_top_04956_UG.dck");
    let wg_source = include_str!("../../data/limited/fdn_v1/FDN_top_20626_WG.dck");
    let ug = original_fixture(ug_source);
    let wg = original_fixture(wg_source);
    let decks = if swapped { [wg, ug] } else { [ug, wg] };
    let mut child = Command::new(env!("CARGO_BIN_EXE_kernel_limited_env"))
        .arg("--london-mulligans-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut request = json!({"request_type":"reset", "schema_version":4, "request_id":"reset",
        "decks":decks, "episode_id":7, "env_seed":seed,
        "max_physical_decisions":8192, "max_policy_steps":8192})
    .to_string();
    let mut transcript = Sha256::new();
    let mut bottom_choices = 0;
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
        assert_eq!(reply["schema_version"], 4);
        assert_eq!(reply["mulligan_rules"], "london_v1");
        if reply["response_type"] == "terminal" {
            drop(input);
            assert!(child.wait().unwrap().success());
            assert_eq!(
                reply["terminal"]["terminal_classification"], "natural",
                "{reply}"
            );
            assert_eq!(reply["terminal"]["terminal_code"], "natural_game_over");
            assert_eq!(bottom_choices, 4); // P0 takes one, P1 takes two.
            let transcript_sha256 = format!("{:x}", transcript.finalize());
            eprintln!(
                "{}",
                json!({
                    "event": "fdn_london_fixture_game_v1",
                    "seed": seed, "swapped": swapped, "requests": index + 1,
                    "bottom_choices": bottom_choices,
                    "input_fixture_sha256s": {
                        "UG": format!("{:x}", Sha256::digest(ug_source.as_bytes())),
                        "WG": format!("{:x}", Sha256::digest(wg_source.as_bytes()))
                    },
                    "transcript_sha256": transcript_sha256,
                    "terminal": reply["terminal"]
                })
            );
            return (transcript_sha256, reply);
        }
        let actions = reply["decision"]["legal_actions"].as_array().unwrap();
        let first = &actions[0]["semantic"];
        let action_index = match first["action_kind"].as_str().unwrap() {
            "choose_london_mulligan" => {
                let target_count = if first["actor"] == "p0" { 1 } else { 2 };
                usize::from(first["mulligan_count"].as_u64().unwrap() < target_count)
            }
            "choose_london_bottom" => {
                bottom_choices += 1;
                0
            }
            _ => [
                "play_land",
                "cast_spell",
                "choose_attacker_inclusion",
                "choose_blocker_inclusion",
                "pass",
            ]
            .iter()
            .find_map(|kind| {
                actions.iter().position(|action| {
                    let semantic = &action["semantic"];
                    semantic["action_kind"] == *kind
                        && (!kind.ends_with("inclusion") || semantic["include"] == true)
                })
            })
            .unwrap_or(0),
        };
        request = request_step(&reply, &format!("step-{index}"), action_index);
    }
    child.kill().unwrap();
    child.wait().unwrap();
    panic!("original fixtures did not finish within the correctness bound");
}

#[test]
#[cfg(feature = "limited-fdn-fixtures")]
fn original_fixtures_play_with_london_in_both_seats_and_replay_exactly() {
    let first = external_fixture_game(false, 123);
    assert_eq!(first, external_fixture_game(false, 123));
    external_fixture_game(true, 701);
}
