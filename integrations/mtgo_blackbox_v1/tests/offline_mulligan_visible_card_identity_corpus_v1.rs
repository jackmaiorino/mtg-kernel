use serde_json::Value;
use std::collections::{HashMap, HashSet};

const CORPUS: &str = include_str!(
    "../fixtures/offline_mulligan_visible_card_identity_corpus_20260810_v1.json"
);

#[test]
fn mulligan_visible_card_identity_corpus_is_complete_and_internally_consistent() {
    let record: Value = serde_json::from_str(CORPUS).unwrap();
    assert_eq!(record["schema_version"], 1);
    assert_eq!(record["distinct_games"], 2);
    assert_eq!(record["sample_count"], 14);
    assert_eq!(record["visible_position_count"], 98);
    assert_eq!(
        record["profile_commitment_sha256"],
        "0310ba629a4caf1fca9806f1e135d15bd3ea99417b2c6b92ec79f948ecbe6739"
    );

    let samples = record["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 14);
    for field in [
        "sample_id",
        "manifest_sha256",
        "canonical_bgra8_sha256",
        "ladder_commitment_sha256",
        "candidate_commitment_sha256",
    ] {
        let values: HashSet<_> = samples
            .iter()
            .map(|sample| sample[field].as_str().unwrap())
            .collect();
        assert_eq!(values.len(), samples.len(), "duplicate field: {field}");
    }

    let mut keep_size_counts = HashMap::new();
    let mut visible_positions = 0_u64;
    for sample in samples {
        assert_eq!(sample["classification"], "match");
        assert_eq!(sample["visible_hand_count"], 7);
        assert_eq!(sample["matched_identity_count"], 7);
        let keep_size = sample["prospective_keep_size"].as_u64().unwrap();
        assert!((1..=7).contains(&keep_size));
        *keep_size_counts.entry(keep_size).or_insert(0_u64) += 1;

        let names = sample["ordered_visible_card_names"].as_array().unwrap();
        assert_eq!(names.len(), 7);
        for name in names {
            assert!(matches!(name.as_str(), Some("Plains" | "Island")));
        }
        visible_positions += names.len() as u64;
    }
    assert_eq!(visible_positions, 98);
    for keep_size in 1..=7 {
        assert_eq!(keep_size_counts.get(&keep_size), Some(&2));
    }
    assert!(record["nonclaim"]
        .as_str()
        .unwrap()
        .contains("not an accuracy estimate"));
}

#[test]
fn mulligan_visible_card_identity_corpus_grants_no_runtime_authority() {
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
