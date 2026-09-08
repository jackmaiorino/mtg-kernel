use serde_json::Value;
use std::collections::HashSet;

const CORPUS: &str =
    include_str!("../fixtures/offline_opening_hand_classifier_corpus_20260810_v1.json");

#[test]
fn two_game_opening_hand_corpus_is_unique_and_internally_consistent() {
    let record: Value = serde_json::from_str(CORPUS).unwrap();
    assert_eq!(record["schema_version"], 1);
    assert_eq!(record["distinct_games"], 2);
    assert_eq!(record["sample_count"], 5);
    assert_eq!(
        record["profile_commitment_sha256"],
        "f57076b8e73261a07fc71a80f6b54eaba0bcae725aa1980869a95f6dbc0392e6"
    );

    let samples = record["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 5);
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

    let mut true_positive = 0_u64;
    let mut true_negative = 0_u64;
    let mut false_positive = 0_u64;
    let mut false_negative = 0_u64;
    for sample in samples {
        let expected = sample["human_label"] == "opening_hand_seven";
        let observed = sample["classification"] == "match";
        match (expected, observed) {
            (true, true) => true_positive += 1,
            (false, false) => true_negative += 1,
            (false, true) => false_positive += 1,
            (true, false) => false_negative += 1,
        }
        assert!(matches!(
            sample["matched_region_count"].as_u64(),
            Some(0..=4)
        ));
    }
    assert_eq!(record["confusion"]["true_positive"], true_positive);
    assert_eq!(record["confusion"]["true_negative"], true_negative);
    assert_eq!(record["confusion"]["false_positive"], false_positive);
    assert_eq!(record["confusion"]["false_negative"], false_negative);
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
        assert_eq!(
            record[field], false,
            "corpus authority must remain false: {field}"
        );
    }
}
