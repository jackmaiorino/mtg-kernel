//! Pauper, Legacy and Vintage catalog: the frozen Pauper prefix, the Standard
//! registry unchanged, then `data/eternal_v1/cards_v1.json`. Snapshot decks
//! whose whole mainboard is Full resolve and play to natural terminals.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardCapability, CARD_DEFS, KERNEL_CARDDB_HASH,
};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1, LimitedJsonlServerV1};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

/// Snapshot archetypes the eternal build plays, by format. A wave that
/// completes a deck adds it here; a regression removes one and fails.
const PLAYABLE: [(&str, &str); 5] = [
    (
        "pauper",
        "pauper-blue-terror-2b4f6710-ccb2-4660-b5f9-ea85c8875ec5",
    ),
    ("pauper", "elves-67156d3a-d400-4269-8400-3a0258a274b3"),
    ("pauper", "pauper-spy-combo"),
    ("pauper", "pauper-jund-wildfire"),
    ("pauper", "pauper-dimir-terror"),
];

fn read_json(relative: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn registry_names(relative: &str) -> Vec<String> {
    read_json(relative)["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|card| card["name"].as_str().unwrap().to_owned())
        .collect()
}

fn fully_supported(name: &str) -> bool {
    card_id_by_name(name)
        .is_some_and(|id| CARD_DEFS[id as usize].capability == CardCapability::Full)
}

/// (format, archetype, mainboard) for every snapshot deck.
fn snapshot_decks() -> Vec<(String, String, CustomDeckV1)> {
    let snapshot = read_json("data/eternal_v1/meta_snapshot_mtggoldfish_20261026.json");
    let mut decks = Vec::new();
    for (format, archetypes) in snapshot["formats"].as_object().unwrap() {
        for archetype in archetypes.as_array().unwrap() {
            let cards = archetype["mainboard"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| CustomCardCountV1 {
                    name: row[1].as_str().unwrap().to_owned(),
                    count: row[0].as_u64().unwrap() as _,
                })
                .collect();
            decks.push((
                format.clone(),
                archetype["archetype"].as_str().unwrap().to_owned(),
                CustomDeckV1 { cards },
            ));
        }
    }
    decks
}

#[test]
fn eternal_registry_follows_the_pauper_and_standard_ids() {
    let pauper = registry_names("data/cards_v1.json");
    let standard = registry_names("data/standard/magezero_v1/cards_v1.json");
    let eternal = registry_names("data/eternal_v1/cards_v1.json");
    let expected: Vec<&String> = pauper.iter().chain(&standard).chain(&eternal).collect();
    assert_eq!(CARD_DEFS.len(), expected.len());
    for (id, name) in expected.into_iter().enumerate() {
        assert_eq!(CARD_DEFS[id].name, name, "id {id}");
    }
    assert!(CARD_DEFS
        .iter()
        .all(|def| def.capability == CardCapability::Full));
    println!("eternal catalog hash {KERNEL_CARDDB_HASH:#018x}");
}

#[test]
fn playable_snapshot_decks_match_the_tracked_list() {
    let playable: Vec<(String, String)> = snapshot_decks()
        .into_iter()
        .filter(|(_, _, deck)| deck.cards.iter().all(|card| fully_supported(&card.name)))
        .map(|(format, archetype, _)| (format, archetype))
        .collect();
    let expected: Vec<(String, String)> = PLAYABLE
        .iter()
        .map(|(format, archetype)| (format.to_string(), archetype.to_string()))
        .collect();
    assert_eq!(playable, expected);
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

/// Plays one game with a fixed preference over legal actions and returns the
/// transcript hash, action counts and spells cast per seat.
fn play(decks: [CustomDeckV1; 2], seed: u64) -> (String, BTreeMap<String, usize>, [usize; 2]) {
    let mut server = LimitedJsonlServerV1::new_with_london_mulligans_v1();
    let mut request = json!({"schema_version":4,"request_type":"reset","request_id":"reset",
        "decks":decks,"episode_id":seed,"env_seed":seed,
        "max_physical_decisions":16384,"max_policy_steps":32768})
    .to_string();
    let mut transcript = Sha256::new();
    let mut counts = BTreeMap::new();
    let mut casts_by_seat = [0; 2];
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
                reply["terminal"]["terminal_code"], "natural_game_over",
                "seed={seed}: {reply}"
            );
            return (
                format!("{:x}", transcript.finalize()),
                counts,
                casts_by_seat,
            );
        }
        let decision = &reply["decision"];
        let actions = decision["legal_actions"].as_array().unwrap();
        let action = actions.iter().min_by_key(|a| rank(a)).unwrap();
        if action["semantic"]["action_kind"] == "cast_spell" {
            let actor: mtg_kernel::rl::PlayerSeatV1 =
                serde_json::from_value(action["semantic"]["actor"].clone()).unwrap();
            casts_by_seat[matches!(actor, mtg_kernel::rl::PlayerSeatV1::P1) as usize] += 1;
        }
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
fn playable_snapshot_decks_finish_and_replay_through_public_sessions() {
    let decks: Vec<(String, CustomDeckV1)> = snapshot_decks()
        .into_iter()
        .filter(|(format, archetype, _)| PLAYABLE.contains(&(format.as_str(), archetype.as_str())))
        .map(|(_, archetype, deck)| (archetype, deck))
        .collect();
    assert_eq!(decks.len(), PLAYABLE.len());
    for (name, deck) in &decks {
        let ids = deck
            .resolve()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        preflight_fully_supported_deck(&ids).unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
    // Each deck plays the next one, so every deck takes both seats.
    for index in 0..decks.len() {
        let (first, second) = (&decks[index], &decks[(index + 1) % decks.len()]);
        let pair = [first.1.clone(), second.1.clone()];
        let seed = 0x4554_4552_0000 + index as u64;
        let game = play(pair.clone(), seed);
        assert_eq!(game, play(pair, seed), "{} / {}", first.0, second.0);
        assert!(
            game.2.iter().all(|casts| *casts > 0),
            "{} / {}: {:?}",
            first.0,
            second.0,
            game.1
        );
        println!(
            "{} / {} seed={seed} transcript_sha256={} casts_by_seat={:?}",
            first.0, second.0, game.0, game.2
        );
    }
}
