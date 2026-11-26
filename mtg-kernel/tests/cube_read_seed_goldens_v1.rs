//! Seed goldens for the Python port in tools/cube_v1/seeds_v1.py (D7, read plan v2, change 11).
//! The constants were computed by the Python port; this test asserts the Rust gives the same.
//! `GameRetry` is the variant m4-interfaces/v2 adds (D5); until D5 lands, its rows are skipped.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{CARD_DEFS, KERNEL_CARDDB_HASH};
use mtg_kernel::cube_draft_v1::{
    deck_seed, derive_seed, game_seed, pod_game_seats, SeedStream, SESSION_SCHEMA_VERSION,
};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1, LimitedJsonlServerV1};
use mtg_kernel::rl::build_deck_pair_state;
use mtg_kernel::state::SplitMix64;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[test]
fn splitmix64_matches_the_published_vector() {
    let mut rng = SplitMix64::seed(1_234_567);
    let expected: [u64; 5] = [
        6457827717110365317,
        3203168211198807973,
        9817491932198370423,
        4593380528125082431,
        16408922859458223821,
    ];
    for value in expected {
        assert_eq!(rng.next_u64(), value);
    }
}

#[test]
fn derive_seed_matches_the_python_port() {
    let cases: [(u64, SeedStream, u64, u64); 25] = [
        (0, SeedStream::Game, 0, 17342717373898743084),
        (0, SeedStream::Game, 27, 1436214522166747746),
        (0, SeedStream::GameCoin, 0, 6412058593177545804),
        (0, SeedStream::GameEnv, 0, 6450973617022139783),
        (0, SeedStream::GameActions, 0, 6327371770725502258),
        (1, SeedStream::Game, 0, 4107345111158512935),
        (1, SeedStream::Game, 27, 9862851748266521902),
        (1, SeedStream::GameCoin, 0, 5713607585791298340),
        (1, SeedStream::GameEnv, 0, 11806238557170168938),
        (1, SeedStream::GameActions, 0, 3130641507755530477),
        (81985529216486895, SeedStream::Game, 0, 149832123990471848),
        (81985529216486895, SeedStream::Game, 27, 6304134740434610908),
        (
            81985529216486895,
            SeedStream::GameCoin,
            0,
            14797589963287401808,
        ),
        (
            81985529216486895,
            SeedStream::GameEnv,
            0,
            4396725098881826446,
        ),
        (
            81985529216486895,
            SeedStream::GameActions,
            0,
            1395649033028093445,
        ),
        (
            9223372036854775807,
            SeedStream::Game,
            0,
            7132182498610909436,
        ),
        (
            9223372036854775807,
            SeedStream::Game,
            27,
            11139677046640204592,
        ),
        (
            9223372036854775807,
            SeedStream::GameCoin,
            0,
            8521563237094384751,
        ),
        (
            9223372036854775807,
            SeedStream::GameEnv,
            0,
            10539902046128495598,
        ),
        (
            9223372036854775807,
            SeedStream::GameActions,
            0,
            6784129164960503782,
        ),
        (
            18446744073709551615,
            SeedStream::Game,
            0,
            13278693861982554827,
        ),
        (
            18446744073709551615,
            SeedStream::Game,
            27,
            8801077725902905928,
        ),
        (
            18446744073709551615,
            SeedStream::GameCoin,
            0,
            16763472772232297675,
        ),
        (
            18446744073709551615,
            SeedStream::GameEnv,
            0,
            3346255405197952324,
        ),
        (
            18446744073709551615,
            SeedStream::GameActions,
            0,
            15045546485419571670,
        ),
    ];
    for (parent, stream, index, expected) in cases {
        assert_eq!(derive_seed(parent, stream, index), expected);
    }
    for (pod, game, expected) in [
        (0, 27, 1436214522166747746),
        (1, 27, 9862851748266521902),
        (81985529216486895, 27, 6304134740434610908),
        (9223372036854775807, 27, 11139677046640204592),
        (18446744073709551615, 27, 8801077725902905928),
    ] {
        assert_eq!(game_seed(pod, game), expected);
    }
}

#[test]
fn pod_game_seats_match_the_python_port() {
    let cases: [(u64, usize, [usize; 2]); 9] = [
        (0, 0, [1, 0]),
        (0, 5, [0, 6]),
        (0, 27, [7, 6]),
        (1, 0, [1, 0]),
        (1, 5, [0, 6]),
        (1, 27, [7, 6]),
        (81985529216486895, 0, [0, 1]),
        (81985529216486895, 5, [0, 6]),
        (81985529216486895, 27, [6, 7]),
    ];
    for (pod, game, expected) in cases {
        assert_eq!(pod_game_seats(pod, game).expect("game in range"), expected);
    }
}

#[test]
fn build_seeds_match_the_python_port() {
    let cases: [(u64, usize, u64, u64, u64); 8] = [
        (0, 0, 0, 11836887688330508145, 16802208799850847246),
        (0, 0, 1, 11836887688330508145, 17525408142943136994),
        (0, 7, 0, 18276781107646718199, 11027067207131447367),
        (0, 7, 1, 18276781107646718199, 4583750976280949105),
        (1, 0, 0, 6644277660526234832, 7972869706821856550),
        (1, 7, 1, 2227647796303230843, 7427020321021221432),
        (
            81985529216486895,
            0,
            0,
            14699875862816212682,
            4472699201026765713,
        ),
        (
            81985529216486895,
            7,
            1,
            6571773155595980281,
            13959654806316832626,
        ),
    ];
    for (draft_seed, chair, build_k, want_deck, want_build) in cases {
        assert_eq!(deck_seed(draft_seed, chair), want_deck);
        assert_eq!(
            derive_seed(deck_seed(draft_seed, chair), SeedStream::DeckBuild, build_k),
            want_build
        );
    }
}

const A15_GOLDEN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../tools/cube_v1/tests/fixtures/a15_read_b_golden.json"
);
const A15_GOLDEN_SHA256: &str = "1930ce67a7fb8403f6612c6f6af74818375b12fb4299c6d7b465da5abfd992e0";

fn u16_list(value: &Value) -> Vec<u16> {
    value
        .as_array()
        .expect("id array")
        .iter()
        .map(|v| u16::try_from(v.as_u64().expect("u64 id")).expect("u16 id"))
        .collect()
}

fn grouped_deck(ids: &[u16]) -> CustomDeckV1 {
    let mut cards = Vec::new();
    let mut i = 0;
    while i < ids.len() {
        let id = ids[i];
        let run = ids[i..].iter().take_while(|&&c| c == id).count();
        cards.push(CustomCardCountV1 {
            name: CARD_DEFS[usize::from(id)].name.to_owned(),
            count: u32::try_from(run).expect("u32 run"),
        });
        i += run;
    }
    CustomDeckV1 {
        cards,
        companion: None,
    }
}

fn canonical_one_card_rows(ids: &[u16]) -> CustomDeckV1 {
    CustomDeckV1 {
        cards: ids
            .iter()
            .map(|&id| CustomCardCountV1 {
                name: CARD_DEFS[usize::from(id)].name.to_owned(),
                count: 1,
            })
            .collect(),
        companion: None,
    }
}

#[test]
fn a15_frozen_episode_replay_matches_runner() {
    let raw = std::fs::read(A15_GOLDEN_PATH).expect("pinned A1.5 golden fixture");
    let digest = format!("{:x}", Sha256::digest(&raw));
    assert_eq!(digest, A15_GOLDEN_SHA256);
    let doc: Value = serde_json::from_slice(&raw).expect("valid golden json");
    assert_eq!(
        doc["kernel_carddb_hash"].as_str(),
        Some(format!("{KERNEL_CARDDB_HASH:016x}").as_str())
    );
    for section in ["spells", "nonbasic_lands", "basics"] {
        for card in doc["pool"][section].as_array().expect("pool section") {
            let cid = usize::try_from(card["card_id"].as_u64().expect("card_id")).unwrap();
            assert_eq!(
                CARD_DEFS[cid].name,
                card["name"].as_str().expect("name"),
                "pool card id {cid} name must match CARD_DEFS"
            );
        }
    }
    let games = doc["expected"]["games"].as_array().expect("4 games");
    assert_eq!(games.len(), 4);

    for game in games {
        let slot = game["slot"].as_u64().expect("slot");
        let episode = game["episode"].as_u64().expect("episode");
        assert_eq!(episode, slot);
        let g_seed = game["game_seed"].as_u64().expect("game_seed");
        let env_seed = game["env_seed"].as_u64().expect("env_seed");
        let action_seed = game["action_seed"].as_u64().expect("action_seed");
        assert_eq!(derive_seed(g_seed, SeedStream::GameEnv, 0), env_seed);
        assert_eq!(derive_seed(g_seed, SeedStream::GameActions, 0), action_seed);

        let p0_ids = u16_list(&game["p0_deck_ids"]);
        let p1_ids = u16_list(&game["p1_deck_ids"]);
        let a15_decks = [grouped_deck(&p0_ids), grouped_deck(&p1_ids)];
        let canonical_decks = [
            canonical_one_card_rows(&p0_ids),
            canonical_one_card_rows(&p1_ids),
        ];

        // 1. Direct engine opening deal matches the pinned expanded libraries and opening hands.
        let state =
            build_deck_pair_state(env_seed, &p0_ids, &p1_ids).expect("supported cube decks");
        let hand0: Vec<u16> = state.players[0]
            .hand
            .iter()
            .map(|&oid| state.objects.get(oid).card_def)
            .collect();
        let hand1: Vec<u16> = state.players[1]
            .hand
            .iter()
            .map(|&oid| state.objects.get(oid).card_def)
            .collect();
        let lib0: Vec<u16> = state.players[0]
            .library
            .iter()
            .map(|&oid| state.objects.get(oid).card_def)
            .collect();
        let lib1: Vec<u16> = state.players[1]
            .library
            .iter()
            .map(|&oid| state.objects.get(oid).card_def)
            .collect();
        assert_eq!(hand0, u16_list(&game["p0_opening_hand"]));
        assert_eq!(hand1, u16_list(&game["p1_opening_hand"]));
        assert_eq!(lib0, u16_list(&game["p0_remaining_library"]));
        assert_eq!(lib1, u16_list(&game["p1_remaining_library"]));
        let mut full0 = hand0.clone();
        full0.extend_from_slice(&lib0);
        let mut full1 = hand1.clone();
        full1.extend_from_slice(&lib1);
        assert_eq!(full0, u16_list(&game["p0_expanded_library"]));
        assert_eq!(full1, u16_list(&game["p1_expanded_library"]));

        // 2. Full LimitedJsonlServerV1 reset produces byte-identical replies for A1.5 grouped rows
        // and canonical 40 one-card rows.
        let reset = |decks: &[CustomDeckV1; 2]| {
            let mut server = LimitedJsonlServerV1::new_with_public_shroud_v1();
            let req = json!({
                "request_type": "reset",
                "schema_version": SESSION_SCHEMA_VERSION,
                "request_id": format!("a15-slot-{slot}"),
                "decks": decks,
                "episode_id": episode,
                "env_seed": env_seed,
                "max_physical_decisions": 16384,
                "max_policy_steps": 32768,
                "max_stack_objects": 128,
            });
            server.handle_line(&req.to_string())
        };
        let reply_a15 = reset(&a15_decks);
        let reply_canon = reset(&canonical_decks);
        assert_eq!(reply_a15, reply_canon);
        let parsed: Value = serde_json::from_str(&reply_a15).expect("valid json reply");
        assert_eq!(parsed["response_type"].as_str(), Some("decision"));
    }
}
