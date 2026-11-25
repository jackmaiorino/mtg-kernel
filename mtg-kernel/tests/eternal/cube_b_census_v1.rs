//! The cube B-run census: the seed function equals `deal.py`'s `seed`
//! (values pinned from the Python function); the stop-kind normaliser; two
//! runs of a tiny drafted range and of a tiny dealt schedule write identical
//! lines; a census game is the pod game `play_pod_game` plays; schedules
//! with another card database or other caps are refused; a process-death
//! line names the game's seats. Plays a few games under small caps.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::KERNEL_CARDDB_HASH;
use mtg_kernel::cube_b_census_v1::{
    compact_json_list, draft_seed, parse_dealt_schedule, play_drafted_range, pod_seed, seed,
    stop_kind, DraftedPod, GameLine, ProcessDeath, SeedPart, A2_CAPS, GAMES_PER_DRAFT,
};
use mtg_kernel::cube_draft_v1::{
    derive_seed, game_seed, play_pod_game, pod_game_seats, CubeDraftError, CubePool, GameCaps,
    GameEnd, SeedStream, DECK_CARDS, SESSION_SCHEMA_VERSION,
};
use mtg_kernel::limited_session_v1::LimitedJsonlServerV1;
use mtg_kernel::state::SplitMix64;
use serde_json::{json, Value};

const CUBE_LIST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/cube_v1/mtgo_vintage_cube_20261007.json"
);
const GAP_RECORD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/cube_v1/mtgo_vintage_cube_20261007_gaps.json"
);
const NAMESPACE: &str = "census-b-test";
/// Small caps, so the games stay cheap; the census itself runs `A2_CAPS`.
const TINY_CAPS: GameCaps = GameCaps {
    max_decisions: 300,
    max_physical_decisions: 16_384,
    max_policy_steps: 32_768,
    max_stack_objects: None,
};

fn pool() -> CubePool {
    let cube = std::fs::read_to_string(CUBE_LIST).expect("cube list");
    let gaps = std::fs::read_to_string(GAP_RECORD).expect("gap record");
    CubePool::from_json_texts(&cube, &gaps, Some(r#"{"cards": []}"#)).expect("draftable pool")
}

/// Runs `work` on a thread with a large stack, as the census workers do.
fn on_big_stack(work: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .name("cube-b-census-v1".to_owned())
        .stack_size(64 << 20)
        .spawn(work)
        .expect("spawn")
        .join()
        .expect("test thread");
}

#[test]
fn seed_matches_deal_py() {
    use SeedPart::{Int, Str};
    // Each value is `deal.seed(parts)` computed by tools/cube_v1/deal.py.
    assert_eq!(
        seed(&[Str("census-b"), Str("draft"), Int(0)]),
        6_876_268_049_017_365_833
    );
    assert_eq!(
        seed(&[Str("census-b"), Str("pod"), Int(0)]),
        8_664_613_558_681_435_576
    );
    assert_eq!(
        seed(&[Str("census-b"), Str("draft"), Int(35_714)]),
        4_494_057_242_552_921_946
    );
    assert_eq!(
        seed(&[Str("census-b"), Str("pod"), Int(35_714)]),
        1_869_219_729_265_160_679
    );
    assert_eq!(draft_seed("census-b", 0), 6_876_268_049_017_365_833);
    assert_eq!(pod_seed("census-b", 35_714), 1_869_219_729_265_160_679);
    // Python's ensure_ascii escapes, including a surrogate pair and DEL.
    let awkward = "ns \"q\"\\ \u{e9}\u{2014}\u{1F600}\n\u{7f}";
    assert_eq!(
        compact_json_list(&[Str(awkward), Str("draft"), Int(7)]),
        r#"["ns \"q\"\\ \u00e9\u2014\ud83d\ude00\n\u007f","draft",7]"#
    );
    assert_eq!(
        seed(&[Str(awkward), Str("draft"), Int(7)]),
        6_136_870_868_535_695_138
    );
}

#[test]
fn stop_kinds_follow_the_census_normaliser() {
    assert_eq!(
        stop_kind(&GameEnd::Natural {
            winner_seat: None,
            reason: "draw".to_owned()
        }),
        None
    );
    assert_eq!(
        stop_kind(&GameEnd::Terminal {
            code: "engine_halted".to_owned(),
            classification: "engine_halted".to_owned(),
            reason: "engine_halted:InvalidEffectContinuation:source:1234 and source:x source:"
                .to_owned(),
        })
        .as_deref(),
        Some("engine_halted:InvalidEffectContinuation:source:* and source:x source:")
    );
    assert_eq!(
        stop_kind(&GameEnd::DecisionCap).as_deref(),
        Some("driver_decision_cap")
    );
    let long = "\u{e9}".repeat(200);
    let kept = "\u{e9}".repeat(160);
    assert_eq!(
        stop_kind(&GameEnd::Panic {
            message: long.clone()
        }),
        Some(format!("panic: {kept}"))
    );
    assert_eq!(
        stop_kind(&GameEnd::SessionError {
            code: "stale_environment_binding".to_owned(),
            message: long,
        }),
        Some(format!("session error stale_environment_binding: {kept}"))
    );
    assert_eq!(
        ProcessDeath::Exit(0xC000_00FD).stop_kind(),
        "process dies: stack overflow (0xC00000FD)"
    );
    assert_eq!(ProcessDeath::Exit(3).stop_kind(), "process dies: exit 3");
}

fn drafted_lines(pool: &CubePool, start: u64, end: u64) -> Vec<String> {
    let mut lines = Vec::new();
    play_drafted_range(
        pool,
        NAMESPACE,
        start,
        end,
        &TINY_CAPS,
        |line: &GameLine| {
            lines.push(line.to_json_line());
            Ok::<(), CubeDraftError>(())
        },
    )
    .expect("drafted range");
    lines
}

#[test]
fn two_runs_of_a_tiny_drafted_range_write_identical_lines() {
    on_big_stack(|| {
        let pool = pool();
        // The last game of draft 0 and the first two of draft 1.
        let (start, end) = (GAMES_PER_DRAFT - 1, GAMES_PER_DRAFT + 2);
        let first = drafted_lines(&pool, start, end);
        let second = drafted_lines(&pool, start, end);
        assert_eq!(first, second);
        assert_eq!(first.len(), 3);
        for (offset, text) in first.iter().enumerate() {
            let line: Value = serde_json::from_str(text).unwrap();
            let index = start + offset as u64;
            let (draft, game) = (index / GAMES_PER_DRAFT, index % GAMES_PER_DRAFT);
            assert_eq!(line["index"], json!(index));
            assert_eq!(line["draft"], json!(draft));
            assert_eq!(line["pod"], json!(draft));
            assert_eq!(line["game"], json!(game));
            assert_eq!(line["draft_seed"], json!(draft_seed(NAMESPACE, draft)));
            let pod = pod_seed(NAMESPACE, draft);
            assert_eq!(line["pod_seed"], json!(pod));
            assert_eq!(line["game_seed"], json!(game_seed(pod, game as usize)));
            for deck in line["decks"].as_array().unwrap() {
                assert_eq!(deck.as_array().unwrap().len(), DECK_CARDS as usize);
            }
            assert_eq!(line["completed"], json!(line["stop_kind"].is_null()));
            assert!(line["max_stack"].as_u64().is_some());
        }
    });
}

fn replay_stack_peak(pod: &DraftedPod, line: &GameLine, caps: &GameCaps) -> u32 {
    let mut server = LimitedJsonlServerV1::new_with_public_shroud_v1();
    let mut request = json!({
        "request_type": "reset", "schema_version": SESSION_SCHEMA_VERSION,
        "request_id": "reset", "episode_id": line.game, "env_seed": line.env_seed,
        "decks": line.seats.map(|seat| pod.decks[seat].to_session_json()),
        "max_physical_decisions": caps.max_physical_decisions,
        "max_policy_steps": caps.max_policy_steps,
    });
    let mut rng = SplitMix64::seed(derive_seed(line.game_seed, SeedStream::GameActions, 0));
    let mut lengths = Vec::new();
    for step in 0..=caps.max_decisions {
        let raw = server.handle_line(&request.to_string());
        let reply: Value = serde_json::from_str(&raw).unwrap();
        if reply["response_type"] == "terminal" || reply["response_type"] == "error" {
            break;
        }
        assert_eq!(reply["response_type"], "decision", "{reply}");
        let decision = &reply["decision"];
        let stack = decision
            .pointer("/observation/projection/stack")
            .and_then(Value::as_array);
        lengths.push(stack.map_or(0, Vec::len) as u32);
        if step == caps.max_decisions {
            break;
        }
        // Replay the uniform policy's rejection sampling from the same seed.
        let actions = decision["legal_actions"].as_array().unwrap();
        let n = actions.len() as u64;
        let threshold = n.wrapping_neg() % n;
        let mut draw = rng.next_u64();
        while draw < threshold {
            draw = rng.next_u64();
        }
        let action = &actions[(draw % n) as usize];
        request = json!({
            "request_type": "step", "schema_version": SESSION_SCHEMA_VERSION,
            "request_id": format!("step-{}", step + 1),
            "episode_id": decision["episode_id"], "expected_step": decision["step"],
            "selected_index": action["selected_index"], "selected_action_id": action["stable_id"],
        });
    }
    lengths.into_iter().max().unwrap_or(0)
}

#[test]
fn drafted_stack_peaks_match_replayed_decision_observations() {
    on_big_stack(|| {
        let pool = pool();
        let pod = DraftedPod::prepare(&pool, NAMESPACE, 0).unwrap();
        let caps = GameCaps {
            max_decisions: 100,
            ..TINY_CAPS
        };
        let mut saw_stack = false;
        play_drafted_range(&pool, NAMESPACE, 0, 2, &caps, |line| {
            let peak = replay_stack_peak(&pod, line, &caps);
            assert_eq!(line.max_stack, peak, "game {}", line.game);
            assert!(line.to_json_line().contains(&format!(
                r#""policy_steps":{},"max_stack":{peak},"#,
                json!(line.policy_steps)
            )));
            saw_stack |= peak > 0;
            Ok::<(), CubeDraftError>(())
        })
        .unwrap();
        assert!(saw_stack, "the range must exercise a nonempty stack");
    });
}

#[test]
fn a_census_game_is_the_pod_game() {
    on_big_stack(|| {
        let pool = pool();
        let pod = DraftedPod::prepare(&pool, NAMESPACE, 0).unwrap();
        let line = pod.play(5, &TINY_CAPS).unwrap();
        let record = play_pod_game(&pod.decks, pod.pod_seed, 5, &TINY_CAPS).unwrap();
        assert_eq!(line.seats, record.seats);
        assert_eq!(line.env_seed, record.env_seed);
        assert_eq!(line.driver_decisions, Some(record.decisions));
        assert_eq!(line.end, serde_json::to_value(&record.end).unwrap());
        assert_eq!(line.winner_seat, record.winner());
        // A process-death line names the same seats and the game's seeds.
        let death = pod.death(5, ProcessDeath::Exit(0xC000_00FD)).unwrap();
        assert_eq!(death.seats, pod_game_seats(pod.pod_seed, 5).unwrap());
        assert_eq!(death.seats, record.seats);
        assert_eq!(death.env_seed, record.env_seed);
        assert_eq!(death.decks, line.decks);
        assert_eq!(death.max_stack, 0);
        assert_eq!(
            death.stop_kind.as_deref(),
            Some("process dies: stack overflow (0xC00000FD)")
        );
        assert!(pod.death(28, ProcessDeath::Exit(1)).is_err());
    });
}

fn schedule_text(pod: &DraftedPod, hash: &str, caps: &GameCaps) -> String {
    let deck = |seat: usize| {
        json!({"label": format!("seat-{seat}"),
        "mainboard": pod.decks[seat].card_ids(), "sideboard": []})
    };
    let episode = |id: &str, seed: u64, decks: [usize; 2]| {
        json!({"episode": {"id": id, "seed": seed, "starting_player": 0, "learner_seat": 0,
            "registered": [deck(decks[0]), deck(decks[1])],
            "selected": [deck(decks[0]), deck(decks[1])], "postboard": false,
            "max_physical_decisions": caps.max_physical_decisions,
            "max_policy_steps": caps.max_policy_steps},
            "opponent": {"kind": "current"}})
    };
    json!({"schema": "cube_deal_schedule/v1", "namespace": "census-dealt", "run": "b",
        "kernel_carddb_hash": hash,
        "iterations": [{"episodes": [episode("e0", 11, [0, 1]), episode("e1", 12, [1, 0])]},
            {"episodes": [episode("e2", 13, [2, 3])]}]})
    .to_string()
}

#[test]
fn two_runs_of_a_tiny_dealt_schedule_write_identical_lines() {
    on_big_stack(|| {
        let pool = pool();
        let pod = DraftedPod::prepare(&pool, NAMESPACE, 0).unwrap();
        let hash = format!("{KERNEL_CARDDB_HASH:016x}");
        let text = schedule_text(&pod, &hash, &TINY_CAPS);
        let schedule = parse_dealt_schedule(&text, &hash, &TINY_CAPS).unwrap();
        assert_eq!(schedule.episodes.len(), 3);
        assert_eq!(
            (schedule.episodes[2].iteration, schedule.episodes[2].slot),
            (1, 0)
        );
        let play = || -> Vec<String> {
            schedule
                .episodes
                .iter()
                .enumerate()
                .map(|(index, episode)| episode.play(index as u64, &TINY_CAPS).to_json_line())
                .collect()
        };
        let first = play();
        assert_eq!(first, play());
        let line: Value = serde_json::from_str(&first[1]).unwrap();
        assert_eq!(line["mode"], "dealt");
        assert_eq!(line["episode_id"], "e1");
        assert_eq!(line["game_seed"], 12);
        assert_eq!(line["seats"], json!([0, 1]));
        let mut deck_one = pod.decks[1].card_ids();
        deck_one.sort_unstable();
        assert_eq!(line["decks"][0], json!(deck_one));

        // Another card database, or caps other than the run's, are refused.
        assert!(parse_dealt_schedule(&text, "0000000000000000", &TINY_CAPS).is_err());
        assert!(parse_dealt_schedule(&text, &hash, &A2_CAPS).is_ok());
        let other = GameCaps {
            max_policy_steps: 65_536,
            ..TINY_CAPS
        };
        assert!(parse_dealt_schedule(&text, &hash, &other)
            .unwrap_err()
            .contains("max_policy_steps"));
        // The session starts p0 and plays the default opening and mulligans,
        // so a schedule asking otherwise is refused.
        let flipped = text.replace("\"starting_player\":0", "\"starting_player\":1");
        assert_ne!(flipped, text);
        assert!(parse_dealt_schedule(&flipped, &hash, &TINY_CAPS)
            .unwrap_err()
            .contains("starting_player"));
        let mut stamped: Value = serde_json::from_str(&text).unwrap();
        stamped["iterations"][0]["episodes"][0]["episode"]["mulligan_modes"] =
            json!(["london", "london"]);
        assert!(
            parse_dealt_schedule(&stamped.to_string(), &hash, &TINY_CAPS)
                .unwrap_err()
                .contains("mulligan_modes")
        );
    });
}
