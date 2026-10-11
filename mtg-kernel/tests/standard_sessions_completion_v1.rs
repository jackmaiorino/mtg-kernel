//! Unchanged MageZero decks through the public JSONL decision contract.
#![cfg(feature = "standard-magezero-fixtures")]
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1, LimitedJsonlServerV1};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

fn fixtures() -> Vec<(String, CustomDeckV1)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../data/standard/magezero_v1/decks");
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|p| p.unwrap().path())
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap();
            let cards = text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(|line| {
                    assert!(
                        !line.starts_with("SB:"),
                        "source fixtures must retain empty sideboards"
                    );
                    let (count, rest) = line.split_once(' ').unwrap();
                    let name = rest.split_once("] ").map_or(rest, |(_, name)| name);
                    CustomCardCountV1 {
                        name: name.trim().into(),
                        count: count.parse().unwrap(),
                    }
                })
                .collect();
            (
                path.file_stem().unwrap().to_string_lossy().into_owned(),
                CustomDeckV1 { cards },
            )
        })
        .collect()
}

fn rank(action: &Value) -> u8 {
    let s = &action["semantic"];
    match s["action_kind"].as_str().unwrap() {
        "play_land" => 0,
        "cast_spell" => 1,
        "choose_attacker_inclusion" | "choose_blocker_inclusion" if s["include"] == true => 2,
        "choose_combat_damage_range" if s["upper_half"] == false => 3,
        "pass" => 10,
        "activate_mana_ability" | "activate_ability" => 11,
        _ => 5,
    }
}

fn play(decks: [CustomDeckV1; 2], seed: u64) -> (String, Value, BTreeMap<String, usize>) {
    let mut server = LimitedJsonlServerV1::new_with_london_mulligans_v1();
    let mut request = json!({"schema_version":4,"request_type":"reset","request_id":"reset",
        "decks":decks,"episode_id":seed,"env_seed":seed,
        "max_physical_decisions":16384,"max_policy_steps":32768})
    .to_string();
    let mut transcript = Sha256::new();
    let mut counts = BTreeMap::new();
    for index in 0..32768 {
        let raw = server.handle_line(&request);
        transcript.update(request.as_bytes());
        transcript.update(b"\n");
        transcript.update(raw.as_bytes());
        transcript.update(b"\n");
        let reply: Value = serde_json::from_str(&raw).unwrap();
        assert_ne!(
            reply["response_type"], "error",
            "seed={seed} step={index}: {reply}"
        );
        if reply["response_type"] == "terminal" {
            assert_eq!(
                reply["terminal"]["terminal_classification"], "natural",
                "seed={seed}: {reply}"
            );
            assert_eq!(
                reply["terminal"]["terminal_code"], "natural_game_over",
                "seed={seed}: {reply}"
            );
            return (format!("{:x}", transcript.finalize()), reply, counts);
        }
        let decision = &reply["decision"];
        let actions = decision["legal_actions"].as_array().unwrap();
        let action = actions.iter().min_by_key(|a| rank(a)).unwrap();
        *counts
            .entry(
                action["semantic"]["action_kind"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            )
            .or_default() += 1;
        request =
            json!({"schema_version":4,"request_type":"step","request_id":format!("step-{index}"),
            "episode_id":decision["episode_id"],"expected_step":decision["step"],
            "selected_index":action["selected_index"],"selected_action_id":action["stable_id"]})
            .to_string();
    }
    panic!("seed {seed} did not reach a natural terminal");
}

#[test]
fn all_sixteen_source_decks_finish_and_replay_through_public_sessions() {
    let decks = fixtures();
    assert_eq!(decks.len(), 16);
    for (name, deck) in &decks {
        let ids = deck
            .resolve()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(ids.len(), if name == "Standard-MonoU" { 62 } else { 60 });
    }
    for (pair, fixtures) in decks.chunks_exact(2).enumerate() {
        let decks = [fixtures[0].1.clone(), fixtures[1].1.clone()];
        let seed = 0x5354_444d_0000 + pair as u64;
        let first = play(decks.clone(), seed);
        let replay = play(decks, seed);
        assert_eq!(first, replay, "{} / {}", fixtures[0].0, fixtures[1].0);
        assert!(
            first.2.get("cast_spell").copied().unwrap_or_default() > 0,
            "no casting: {:?}",
            first.2
        );
        println!(
            "{} / {} seed={seed} transcript_sha256={} actions={:?}",
            fixtures[0].0, fixtures[1].0, first.0, first.2
        );
    }
}
