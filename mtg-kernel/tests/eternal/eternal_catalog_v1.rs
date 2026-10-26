//! Pauper, Legacy and Vintage catalog: the frozen Pauper prefix, the Standard
//! registry unchanged, then `data/eternal_v1/cards_v1.json`. Snapshot decks
//! whose whole mainboard and companion are Full resolve and play to natural
//! terminals, with both a fixed action preference and seeded random actions.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardCapability, CARD_DEFS, KERNEL_CARDDB_HASH,
};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1, LimitedJsonlServerV1};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Snapshot archetypes the eternal build plays, by format. A wave that
/// completes a deck adds it here; a regression removes one and fails.
/// Companion decks stay out until the engine supports companions.
const PLAYABLE: [(&str, &str); 14] = [
    ("pauper", "pauper-mono-red-madness"),
    (
        "pauper",
        "pauper-blue-terror-2b4f6710-ccb2-4660-b5f9-ea85c8875ec5",
    ),
    ("pauper", "elves-67156d3a-d400-4269-8400-3a0258a274b3"),
    (
        "pauper",
        "pauper-dimir-faeries-625747e4-1ba6-4401-af7b-5c8033a12c6c",
    ),
    ("pauper", "pauper-grixis-affinity"),
    ("pauper", "pauper-spy-combo"),
    ("pauper", "pauper-mono-red-rally"),
    (
        "pauper",
        "pauper-grixis-control-334e14c5-6fe9-46da-9c9b-799ef8bd1f0f",
    ),
    ("pauper", "pauper-jund-wildfire"),
    (
        "pauper",
        "pauper-dimir-control-1e2279c1-d135-4609-8b99-83f2ab98c424",
    ),
    ("pauper", "pauper-dimir-terror"),
    ("pauper", "pauper-grixis-faeries"),
    ("legacy", "legacy-dimir-tempo"),
    ("legacy", "legacy-izzet-delver"),
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

/// Standard definitions precede the eternal registry. A Standard append
/// shifts every eternal id, which is allowed only before an eternal model
/// trains; change this with the eternal catalog version.
const STANDARD_DEFINITIONS: usize = 254;

struct SnapshotDeck {
    format: String,
    archetype: String,
    mainboard: CustomDeckV1,
    companion: Option<String>,
}

/// Every snapshot deck, in snapshot order.
fn snapshot_decks() -> Vec<SnapshotDeck> {
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
            decks.push(SnapshotDeck {
                format: format.clone(),
                archetype: archetype["archetype"].as_str().unwrap().to_owned(),
                mainboard: CustomDeckV1 { cards },
                companion: archetype["companion"].as_str().map(str::to_owned),
            });
        }
    }
    decks
}

#[test]
fn eternal_registry_follows_the_pauper_and_standard_ids() {
    let pauper = registry_names("data/cards_v1.json");
    let standard = registry_names("data/standard/magezero_v1/cards_v1.json");
    let eternal = registry_names("data/eternal_v1/cards_v1.json");
    assert_eq!(standard.len(), STANDARD_DEFINITIONS);
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
    let decks = snapshot_decks();
    let playable: BTreeSet<(String, String)> = decks
        .iter()
        .filter(|deck| {
            deck.mainboard
                .cards
                .iter()
                .map(|card| card.name.as_str())
                .chain(deck.companion.as_deref())
                .all(fully_supported)
        })
        .map(|deck| (deck.format.clone(), deck.archetype.clone()))
        .collect();
    let expected: BTreeSet<(String, String)> = PLAYABLE
        .iter()
        .map(|(format, archetype)| (format.to_string(), archetype.to_string()))
        .collect();
    assert_eq!(playable, expected);
    assert!(decks
        .iter()
        .filter(|deck| PLAYABLE.contains(&(deck.format.as_str(), deck.archetype.as_str())))
        .all(|deck| deck.companion.is_none()));
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

/// Whether the public session can follow this action. Policy observation v5
/// fails closed while an Escape cast stages its graveyard exile (Sleep of the
/// Dead), so these games never begin one; every other action is admitted.
fn session_can_follow(action: &Value) -> bool {
    let s = &action["semantic"];
    if s["action_kind"] != "cast_spell"
        || s["source"]["zone"] != serde_json::to_value(mtg_kernel::state::Zone::Graveyard).unwrap()
    {
        return true;
    }
    let id = s["source"]["card_db_id"].as_u64().unwrap() as usize;
    CARD_DEFS[id].escape.is_none()
}

/// How a test game picks among legal actions. `Ranked` casts and attacks
/// but never activates an ability; `Random` reaches activations, so lands,
/// mana rocks and activated abilities run too.
#[derive(Clone, Copy)]
enum Policy {
    Ranked,
    Random(u64),
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

struct Game {
    transcript_sha256: String,
    action_counts: BTreeMap<String, usize>,
    casts_by_seat: [usize; 2],
}

/// Plays one game to a natural terminal and returns its transcript hash,
/// chosen action kinds and spells cast per seat.
fn play(decks: [CustomDeckV1; 2], seed: u64, policy: Policy) -> Game {
    const MAX_STEPS: usize = 131_072;
    let mut server = LimitedJsonlServerV1::new_with_london_mulligans_v1();
    let mut request = json!({"schema_version":4,"request_type":"reset","request_id":"reset",
        "decks":decks,"episode_id":seed,"env_seed":seed,
        "max_physical_decisions":65536,"max_policy_steps":MAX_STEPS})
    .to_string();
    let mut transcript = Sha256::new();
    let mut action_counts = BTreeMap::new();
    let mut casts_by_seat = [0; 2];
    let mut random_state = match policy {
        Policy::Random(state) => state,
        Policy::Ranked => 0,
    };
    for index in 0..MAX_STEPS {
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
            return Game {
                transcript_sha256: format!("{:x}", transcript.finalize()),
                action_counts,
                casts_by_seat,
            };
        }
        let decision = &reply["decision"];
        let actions: Vec<&Value> = decision["legal_actions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|action| session_can_follow(action))
            .collect();
        let action = match policy {
            Policy::Ranked => *actions.iter().min_by_key(|a| rank(a)).unwrap(),
            Policy::Random(_) => {
                actions[(splitmix64(&mut random_state) % actions.len() as u64) as usize]
            }
        };
        if action["semantic"]["action_kind"] == "cast_spell" {
            let actor: mtg_kernel::rl::PlayerSeatV1 =
                serde_json::from_value(action["semantic"]["actor"].clone()).unwrap();
            casts_by_seat[matches!(actor, mtg_kernel::rl::PlayerSeatV1::P1) as usize] += 1;
        }
        *action_counts
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
    let decks: Vec<SnapshotDeck> = snapshot_decks()
        .into_iter()
        .filter(|deck| PLAYABLE.contains(&(deck.format.as_str(), deck.archetype.as_str())))
        .collect();
    assert_eq!(decks.len(), PLAYABLE.len());
    for deck in &decks {
        let ids = deck
            .mainboard
            .resolve()
            .unwrap_or_else(|error| panic!("{}: {error}", deck.archetype));
        preflight_fully_supported_deck(&ids)
            .unwrap_or_else(|error| panic!("{}: {error:?}", deck.archetype));
    }
    // Each deck plays the next one, so every deck takes both seats, once
    // with ranked actions and once with seeded random actions.
    let mut random_activations = 0;
    for index in 0..decks.len() {
        let (first, second) = (&decks[index], &decks[(index + 1) % decks.len()]);
        let pair = [first.mainboard.clone(), second.mainboard.clone()];
        let seed = 0x4554_4552_0000 + index as u64;
        for policy in [Policy::Ranked, Policy::Random(seed)] {
            let game = play(pair.clone(), seed, policy);
            let replay = play(pair.clone(), seed, policy);
            assert_eq!(
                game.transcript_sha256, replay.transcript_sha256,
                "{} / {}",
                first.archetype, second.archetype
            );
            if let Policy::Ranked = policy {
                assert!(
                    game.casts_by_seat.iter().all(|casts| *casts > 0),
                    "{} / {}: {:?}",
                    first.archetype,
                    second.archetype,
                    game.action_counts
                );
            } else {
                random_activations += game.action_counts.get("activate_ability").unwrap_or(&0);
            }
            println!(
                "{} / {} seed={seed} random={} transcript_sha256={} actions={:?}",
                first.archetype,
                second.archetype,
                matches!(policy, Policy::Random(_)),
                game.transcript_sha256,
                game.action_counts
            );
        }
    }
    assert!(
        random_activations > 0,
        "random games activated no abilities"
    );
}
