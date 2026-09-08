use serde_json::Value;
use std::collections::HashSet;

const CORPUS: &str =
    include_str!("../fixtures/offline_fixed_basic_deck_identity_corpus_20260810_v1.json");

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[test]
fn fixed_deck_and_visible_identity_sequence_are_internally_consistent() {
    let record: Value = serde_json::from_str(CORPUS).unwrap();
    assert_eq!(record["schema_version"], 1);
    assert_eq!(record["deck"]["total_cards"], 60);
    let cards = record["deck"]["cards"].as_array().unwrap();
    assert_eq!(cards.len(), 2);
    assert_eq!(cards.iter().map(|card| card["count"].as_u64().unwrap()).sum::<u64>(), 60);
    assert_eq!(
        cards
            .iter()
            .map(|card| card["visible_card_name"].as_str().unwrap())
            .collect::<HashSet<_>>(),
        HashSet::from(["Plains", "Island"])
    );

    for field in [
        "bottom_six_state_profile_commitment_sha256",
        "visible_card_profile_commitment_sha256",
    ] {
        assert!(is_lower_sha256(record[field].as_str().unwrap()));
    }

    let samples = record["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 8);
    let mut ids = HashSet::new();
    let mut manifests = HashSet::new();
    let mut frames = HashSet::new();
    for sample in samples {
        assert_eq!(sample["state_classification"], "match");
        assert_eq!(
            sample["visible_hand_count"].as_u64().unwrap()
                + sample["selected_count"].as_u64().unwrap(),
            7
        );
        assert!(ids.insert(sample["sample_id"].as_str().unwrap()));
        for (field, set) in [
            ("manifest_sha256", &mut manifests),
            ("frame_sha256", &mut frames),
        ] {
            let digest = sample[field].as_str().unwrap();
            assert!(is_lower_sha256(digest));
            assert!(set.insert(digest));
        }
        assert!(is_lower_sha256(
            sample["state_commitment_sha256"].as_str().unwrap()
        ));
        assert!(is_lower_sha256(
            sample["identity_commitment_sha256"].as_str().unwrap()
        ));
        for card_name in sample["visible_card_names"].as_array().unwrap() {
            assert!(matches!(card_name.as_str().unwrap(), "Island" | "Plains"));
        }
    }

    let first_game: Vec<_> = samples
        .iter()
        .filter(|sample| sample["sample_id"].as_str().unwrap().starts_with("game-958670600"))
        .collect();
    assert_eq!(first_game.len(), 7);
    for (selected, sample) in first_game.iter().enumerate() {
        assert_eq!(sample["selected_count"], selected as u64);
        assert_eq!(sample["done_visible"], selected == 6);
        if selected < 6 {
            assert_eq!(sample["identity_classification"], "match");
            assert_eq!(sample["matched_identity_count"], (7 - selected) as u64);
        } else {
            assert_eq!(sample["identity_classification"], "no_match");
            assert_eq!(sample["matched_identity_count"], 0);
            assert!(sample["visible_limitation"]
                .as_str()
                .unwrap()
                .contains("dims"));
        }
    }
    for pair in first_game[..6].windows(2) {
        let before = pair[0]["visible_card_names"].as_array().unwrap();
        let after = pair[1]["visible_card_names"].as_array().unwrap();
        assert_eq!(after, &before[1..]);
    }

    let initial_hands: Vec<_> = samples
        .iter()
        .filter(|sample| sample["selected_count"] == 0)
        .collect();
    assert_eq!(initial_hands.len(), 2);
    assert!(initial_hands.iter().all(|sample| {
        sample["identity_classification"] == "match"
            && sample["matched_identity_count"] == 7
    }));
    assert_eq!(record["measurement"]["initial_hand_cards_identified"], 14);
    assert_eq!(record["measurement"]["direct_cards_identified"], 34);
}

#[test]
fn fixed_deck_identity_corpus_grants_no_runtime_authority() {
    let record: Value = serde_json::from_str(CORPUS).unwrap();
    assert!(record["nonclaim"]
        .as_str()
        .unwrap()
        .contains("not an accuracy estimate"));
    for field in [
        "safe_for_live_frame",
        "safe_for_semantic_evidence",
        "safe_for_observation_v5",
        "safe_for_policy_scoring",
        "safe_for_input",
    ] {
        assert_eq!(record[field], false);
    }
}
