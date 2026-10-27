//! Pauper, Legacy and Vintage catalog: the frozen Pauper prefix, the Standard
//! registry unchanged, then `data/eternal_v1/cards_v1.json`. Snapshot decks
//! whose whole mainboard and companion are Full resolve and play to natural
//! terminals, with both a fixed action preference and seeded random actions.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardCapability, TargetSpec, CARD_DEFS,
    KERNEL_CARDDB_HASH,
};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1, LimitedJsonlServerV1};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Snapshot archetypes the eternal build plays, by format. A wave that
/// completes a deck adds it here; a regression removes one and fails. A
/// deck's companion is part of the deck the session plays.
const PLAYABLE: [(&str, &str); 57] = [
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
    ("pauper", "tron-c244ab01-c9e3-400d-8db1-6d717616b7e2"),
    ("pauper", "pauper-esper-affinity"),
    ("pauper", "pauper-esper-ephemerate"),
    (
        "pauper",
        "pauper-mono-blue-faeries-f7d232c7-2c3c-4459-b219-f3e8d9abbec6",
    ),
    ("pauper", "pauper-dimir-affinity"),
    ("pauper", "pauper-mardu-synthesizer"),
    ("pauper", "pauper-4c-affinity"),
    ("pauper", "pauper-azorius-gates"),
    ("pauper", "bogles-df219f8f-1b57-437c-b4d8-dd4a5c7bdd7c"),
    ("pauper", "pauper-white-aggro"),
    ("pauper", "pauper-rakdos-madness"),
    ("pauper", "pauper-boros-moxite"),
    (
        "pauper",
        "pauper-mono-white-aggro-6b67ea64-5bde-42ea-8d1a-93578d3e3f3c",
    ),
    ("pauper", "pauper-snacker-gates"),
    ("pauper", "pauper-boros-synthesizer"),
    ("pauper", "pauper-azorius-faeries"),
    ("legacy", "legacy-dimir-tempo"),
    ("legacy", "legacy-izzet-delver"),
    ("legacy", "legacy-doomsday"),
    ("legacy", "legacy-sneak-and-show"),
    ("legacy", "legacy-aluren"),
    ("legacy", "legacy-omni-tell"),
    ("legacy", "legacy-temur-delver"),
    ("legacy", "legacy-boros-energy"),
    ("legacy", "legacy-beanstalk-control-non-yorion"),
    (
        "legacy",
        "legacy-azorius-tempo-9bd90024-e52f-4284-821d-f037ed910a44",
    ),
    (
        "legacy",
        "legacy-jeskai-tempo-7f301e99-6af3-4ead-a85a-5ebb92d56c0f",
    ),
    ("legacy", "legacy-esper-control"),
    ("legacy", "legacy-azorius-tempo"),
    ("legacy", "legacy-mystic-forge-combo"),
    ("legacy", "legacy-eldrazi"),
    ("legacy", "legacy-tron"),
    ("vintage", "vintage-jewel-shops"),
    ("vintage", "vintage-mono-white-initiative"),
    ("vintage", "vintage-esper-lurrus-control"),
    ("vintage", "vintage-dimir-lurrus-control"),
    ("vintage", "vintage-lurrus-po"),
    (
        "vintage",
        "vintage-lurrus-breach-9d1cc5d1-52c8-436c-8711-ee292b96e9d4",
    ),
    ("vintage", "vintage-jeskai-lurrus"),
    ("vintage", "vintage-4c-psychic-frog"),
    ("vintage", "vintage-raker-shops"),
    ("vintage", "vintage-oath-of-druids"),
    ("vintage", "vintage-paradoxical-outcome"),
    ("vintage", "vintage-blue-tinker"),
    ("vintage", "vintage-beseech-storm"),
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
    /// The mainboard and the companion, as the session takes them.
    deck: CustomDeckV1,
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
                deck: CustomDeckV1 {
                    cards,
                    companion: archetype["companion"].as_str().map(str::to_owned),
                },
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
            deck.deck
                .cards
                .iter()
                .map(|card| card.name.as_str())
                .chain(deck.deck.companion.as_deref())
                .all(fully_supported)
        })
        .map(|deck| (deck.format.clone(), deck.archetype.clone()))
        .collect();
    let expected: BTreeSet<(String, String)> = PLAYABLE
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

/// Whether `seat` controls a battlefield permanent named `name` in the
/// decision's public projection.
fn controls_named(decision: &Value, seat: &Value, name: &str) -> bool {
    decision["observation"]["projection"]["battlefield"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|cards| cards.as_array().into_iter().flatten())
        .any(|card| {
            &card["stable"]["controller"] == seat
                && card["stable"]["card_db_id"]
                    .as_u64()
                    .is_some_and(|id| CARD_DEFS[id as usize].name == name)
        })
}

/// Whether the opponent of `seat` controls a Hexing Squelcher, whose own
/// "Ward--Pay 2 life" and the one it grants their other creatures policy
/// observation v5 does not represent.
fn opponent_has_life_ward(decision: &Value, seat: &Value) -> bool {
    decision["observation"]["projection"]["battlefield"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|cards| cards.as_array().into_iter().flatten())
        .any(|card| {
            &card["stable"]["controller"] != seat
                && card["stable"]["card_db_id"]
                    .as_u64()
                    .is_some_and(|id| CARD_DEFS[id as usize].name == "Hexing Squelcher")
        })
}

/// Whether starting this spell or ability may target a permanent: its card
/// or ability has a target, or its entry trigger targets one.
fn may_target(action: &Value) -> bool {
    let s = &action["semantic"];
    let Some(id) = s["source"]["card_db_id"].as_u64() else {
        return false;
    };
    let def = &CARD_DEFS[id as usize];
    match s["action_kind"].as_str() {
        Some("cast_spell") => {
            def.target_spec != TargetSpec::None
                || def.mode2.is_some()
                || matches!(def.name, "Portable Hole" | "Witch Enchanter")
        }
        Some("activate_ability") => s["ability_index"]
            .as_u64()
            .and_then(|index| def.activated_abilities.get(index as usize))
            .is_some_and(|ability| ability.target_spec != TargetSpec::None),
        _ => false,
    }
}

/// Whether this action puts a companion into its owner's hand.
fn is_companion_action(action: &Value) -> bool {
    let s = &action["semantic"];
    s["action_kind"] == "activate_ability"
        && s["source"]["zone"] == serde_json::to_value(mtg_kernel::state::Zone::Exile).unwrap()
        && s["source"]["card_db_id"]
            .as_u64()
            .is_some_and(|id| CARD_DEFS[id as usize].name == "Lurrus of the Dream-Den")
}

/// Whether the public session can follow this action. Policy observation v5
/// fails closed while an Escape cast stages its graveyard exile (Sleep of the
/// Dead, and every nonland card under Underworld Breach) and once an extra
/// turn is scheduled (Emrakul, the Aeons Torn's cast trigger, Time Walk,
/// Time Vault's ability), so these games never begin either; the engine
/// paths are covered by the rules tests. Every other action is admitted.
fn session_can_follow(action: &Value, decision: &Value) -> bool {
    let s = &action["semantic"];
    if !matches!(
        s["action_kind"].as_str(),
        Some("cast_spell" | "activate_ability")
    ) {
        return true;
    }
    let Some(id) = s["source"]["card_db_id"].as_u64() else {
        return true;
    };
    let id = id as usize;
    if matches!(
        CARD_DEFS[id].name,
        "Emrakul, the Aeons Torn" | "Time Walk" | "Time Vault"
    ) {
        return false;
    }
    s["action_kind"] != "cast_spell"
        || s["source"]["zone"] != serde_json::to_value(mtg_kernel::state::Zone::Graveyard).unwrap()
        || (CARD_DEFS[id].escape.is_none()
            && !controls_named(decision, &s["actor"], "Underworld Breach"))
}

/// Whether this action chooses a Ward permanent as a target.
fn targets_ward_permanent(action: &Value) -> bool {
    let target = &action["semantic"]["target"];
    target["target_kind"] == "object"
        && target["object"]["card_db_id"]
            .as_u64()
            .is_some_and(|id| CARD_DEFS[id as usize].ward_cost.is_some())
}

/// Whether any public stack item's targets include a Ward permanent.
fn public_targets_ward_permanent(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            map.get("targets")
                .and_then(Value::as_array)
                .is_some_and(|targets| {
                    targets.iter().any(|target| {
                        target["target_kind"] == "object"
                            && target["object"]["card_db_id"]
                                .as_u64()
                                .is_some_and(|id| CARD_DEFS[id as usize].ward_cost.is_some())
                    })
                })
                || map.values().any(public_targets_ward_permanent)
        }
        Value::Array(items) => items.iter().any(public_targets_ward_permanent),
        _ => false,
    }
}

/// Whether any public stack item is controlled by `seat`.
fn controls_stack_item(value: &Value, seat: &Value) -> bool {
    match value {
        Value::Object(map) => {
            map.get("stack")
                .and_then(Value::as_array)
                .is_some_and(|items| items.iter().any(|item| &item["controller"] == seat))
                || map.values().any(|child| controls_stack_item(child, seat))
        }
        Value::Array(items) => items.iter().any(|child| controls_stack_item(child, seat)),
        _ => false,
    }
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
    companion_actions: usize,
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
    let mut companion_actions = 0;
    // The last chosen actions, reported if a game fails closed.
    let mut recent: Vec<String> = Vec::new();
    let mut random_state = match policy {
        Policy::Random(state) => state,
        Policy::Ranked => 0,
    };
    // A game that loops without ending fails here with its last actions
    // instead of running until the CI job times out.
    let started = std::time::Instant::now();
    for index in 0..MAX_STEPS {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(600),
            "seed={seed} random={} step={index}: game still running after 600 s\nlast actions:\n{}",
            matches!(policy, Policy::Random(_)),
            recent.join("\n")
        );
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
                reply["terminal"]["terminal_code"],
                "natural_game_over",
                "seed={seed}: {reply}
last actions:
{}",
                recent.join(
                    "
"
                )
            );
            return Game {
                transcript_sha256: format!("{:x}", transcript.finalize()),
                action_counts,
                casts_by_seat,
                companion_actions,
            };
        }
        let decision = &reply["decision"];
        let actions: Vec<&Value> = decision["legal_actions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|action| session_can_follow(action, decision))
            .collect();
        // Policy observation v5 fails closed while two stack items target
        // one Ward permanent, so these games avoid targeting a Ward
        // permanent whenever another action is available, and start no new
        // spell or ability while something on the stack targets one. It
        // does not represent "Ward--Pay N life" at all, so while a seat's
        // opponent controls a Hexing Squelcher that seat starts no spell or
        // ability that may target a permanent.
        let ward_targeted = public_targets_ward_permanent(decision);
        let unwarded: Vec<&Value> = actions
            .iter()
            .copied()
            .filter(|action| {
                if targets_ward_permanent(action) {
                    return false;
                }
                let starts_new = matches!(
                    action["semantic"]["action_kind"].as_str(),
                    Some("cast_spell" | "activate_ability")
                );
                if starts_new
                    && may_target(action)
                    && opponent_has_life_ward(decision, &action["semantic"]["actor"])
                {
                    return false;
                }
                !ward_targeted || !starts_new
            })
            .collect();
        let actions = if unwarded.is_empty() {
            actions
        } else {
            unwarded
        };
        let action = match policy {
            // A ranked seat passes while its own spell or ability is on the
            // stack, so free recasts (Acererak under Aluren) cannot pile up
            // above a trigger forever.
            Policy::Ranked => match actions
                .iter()
                .find(|a| a["semantic"]["action_kind"] == "pass")
            {
                Some(pass) if controls_stack_item(decision, &pass["semantic"]["actor"]) => *pass,
                _ => *actions.iter().min_by_key(|a| rank(a)).unwrap(),
            },
            Policy::Random(_) => {
                actions[(splitmix64(&mut random_state) % actions.len() as u64) as usize]
            }
        };
        if is_companion_action(action) {
            companion_actions += 1;
        }
        recent.push(action["semantic"].to_string());
        if recent.len() > 8 {
            recent.remove(0);
        }
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
        let (ids, companion) = deck
            .deck
            .resolve_with_companion()
            .unwrap_or_else(|error| panic!("{}: {error}", deck.archetype));
        preflight_fully_supported_deck(&ids)
            .unwrap_or_else(|error| panic!("{}: {error:?}", deck.archetype));
        assert_eq!(companion.is_some(), deck.deck.companion.is_some());
    }
    // Each deck plays the next one, so every deck takes both seats, once
    // with ranked actions and once with seeded random actions.
    let mut random_activations = 0;
    let mut companion_actions = 0;
    // Every deck casts a spell in its two ranked games (one per seat); a
    // single shuffle can leave one seat without a castable spell.
    let mut ranked_casts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    for index in 0..decks.len() {
        let (first, second) = (&decks[index], &decks[(index + 1) % decks.len()]);
        let pair = [first.deck.clone(), second.deck.clone()];
        let seed = 0x4554_4552_0000 + index as u64;
        for policy in [Policy::Ranked, Policy::Random(seed)] {
            // Every pairing runs even after one fails, so a single run
            // reports all failing games (each panic message is printed).
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let game = play(pair.clone(), seed, policy);
                let replay = play(pair.clone(), seed, policy);
                assert_eq!(
                    game.transcript_sha256, replay.transcript_sha256,
                    "{} / {}",
                    first.archetype, second.archetype
                );
                game
            }));
            let Ok(game) = outcome else {
                failures.push(format!(
                    "{} / {} seed={seed} random={}",
                    first.archetype,
                    second.archetype,
                    matches!(policy, Policy::Random(_))
                ));
                continue;
            };
            companion_actions += game.companion_actions;
            if let Policy::Ranked = policy {
                *ranked_casts.entry(first.archetype.as_str()).or_default() += game.casts_by_seat[0];
                *ranked_casts.entry(second.archetype.as_str()).or_default() +=
                    game.casts_by_seat[1];
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
        failures.is_empty(),
        "failing games:
{}",
        failures.join(
            "
"
        )
    );
    assert!(
        ranked_casts.values().all(|casts| *casts > 0),
        "{ranked_casts:?}"
    );
    assert!(
        random_activations > 0,
        "random games activated no abilities"
    );
    assert!(
        companion_actions > 0,
        "no game put a companion into its owner's hand"
    );
}
