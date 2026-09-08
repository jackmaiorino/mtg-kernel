use serde_json::Value;
use std::collections::HashSet;

const CORPUS: &str =
    include_str!("../fixtures/offline_bottom_six_state_classifier_corpus_20260810_v1.json");

#[test]
fn all_seven_bottom_six_stages_are_unique_and_consistent() {
    let record: Value = serde_json::from_str(CORPUS).unwrap();
    assert_eq!(record["schema_version"], 1);
    assert_eq!(record["distinct_games"], 2);
    assert_eq!(record["positive_game_count"], 1);
    assert_eq!(record["sample_count"], 9);
    assert_eq!(
        record["profile_commitment_sha256"],
        "72a52b5f1cca1785f7f923780fa4b0a4bf2724f3dcd4c4529df96c0c1d41763b"
    );

    let samples = record["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 9);
    for field in [
        "sample_id",
        "manifest_sha256",
        "canonical_bgra8_sha256",
        "preview_png_sha256",
        "candidate_commitment_sha256",
    ] {
        let values: HashSet<_> = samples
            .iter()
            .map(|sample| sample[field].as_str().unwrap())
            .collect();
        assert_eq!(
            values.len(),
            samples.len(),
            "duplicate corpus field: {field}"
        );
    }

    let mut confusion = [0_u64; 4];
    let mut observed_stages = HashSet::new();
    for sample in samples {
        let expected = sample["human_label"] == "bottom_six_state";
        let observed = sample["classification"] == "match";
        match (expected, observed) {
            (true, true) => confusion[0] += 1,
            (false, false) => confusion[1] += 1,
            (false, true) => confusion[2] += 1,
            (true, false) => confusion[3] += 1,
        }
        assert!(matches!(
            sample["matched_exact_region_count"].as_u64(),
            Some(0..=2)
        ));
        let counts = sample["occupancy_bright_pixel_counts"].as_array().unwrap();
        assert_eq!(counts.len(), 7);
        assert!(counts
            .iter()
            .all(|count| matches!(count.as_u64(), Some(0..=2160))));

        if observed {
            let selected = sample["selected_count"].as_u64().unwrap();
            let hand = sample["visible_hand_count"].as_u64().unwrap();
            let actions = sample["legal_action_count"].as_u64().unwrap();
            assert_eq!(selected + hand, 7);
            assert_eq!(actions, hand + 1);
            assert_eq!(sample["done_visible"], selected == 6);
            assert!(observed_stages.insert(selected));
            for (index, count) in counts.iter().enumerate() {
                assert_eq!(count.as_u64().unwrap() >= 900, (index as u64) < hand);
            }
        } else {
            for field in [
                "selected_count",
                "visible_hand_count",
                "done_visible",
                "legal_action_count",
            ] {
                assert!(sample[field].is_null(), "negative must not expose {field}");
            }
        }
    }
    assert_eq!(observed_stages, (0_u64..=6).collect());
    assert_eq!(record["confusion"]["true_positive"], confusion[0]);
    assert_eq!(record["confusion"]["true_negative"], confusion[1]);
    assert_eq!(record["confusion"]["false_positive"], confusion[2]);
    assert_eq!(record["confusion"]["false_negative"], confusion[3]);
    assert!(record["nonclaim"]
        .as_str()
        .unwrap()
        .contains("not an accuracy estimate"));
    assert!(record["nonclaim"].as_str().unwrap().contains("one game"));
}

#[test]
fn corpus_grants_no_runtime_authority() {
    let record: Value = serde_json::from_str(CORPUS).unwrap();
    for field in [
        "safe_for_live_frame",
        "safe_for_semantic_evidence",
        "safe_for_observation_v5",
        "safe_for_policy_scoring",
        "safe_for_input",
    ] {
        assert_eq!(record[field], false, "authority must remain false: {field}");
    }
}
