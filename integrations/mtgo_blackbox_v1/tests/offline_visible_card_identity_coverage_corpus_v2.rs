use serde_json::Value;
use std::collections::HashSet;

const CORPUS: &str =
    include_str!("../fixtures/offline_visible_card_identity_coverage_corpus_20260810_v2.json");

#[test]
fn partial_heldout_template_coverage_exposes_no_identity_labels() {
    let record: Value = serde_json::from_str(CORPUS).unwrap();
    assert_eq!(record["schema_version"], 2);
    assert_eq!(record["profile_scope"], "one_observed_bottom_six_hand_only");
    assert_eq!(record["profile_template_count"], 7);
    assert_eq!(record["distinct_heldout_games"], 2);
    assert_eq!(record["visible_slot_count"], 14);
    assert_eq!(record["matched_visible_slot_count"], 6);
    assert_eq!(record["full_hand_match_count"], 0);

    let samples = record["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 2);
    let ids: HashSet<_> = samples
        .iter()
        .map(|sample| sample["sample_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), samples.len());
    let total_matches: u64 = samples
        .iter()
        .map(|sample| sample["matched_identity_count"].as_u64().unwrap())
        .sum();
    assert_eq!(total_matches, 6);
    for sample in samples {
        assert_eq!(sample["visible_hand_count"], 7);
        assert_eq!(sample["classification"], "no_match");
        assert_eq!(sample["exposed_identity_count"], 0);
        assert!(sample["matched_identity_count"].as_u64().unwrap() < 7);
    }
    assert!(record["nonclaim"]
        .as_str()
        .unwrap()
        .contains("not an accuracy estimate"));
}

#[test]
fn coverage_corpus_grants_no_runtime_authority() {
    let record: Value = serde_json::from_str(CORPUS).unwrap();
    for field in [
        "safe_for_semantic_evidence",
        "safe_for_observation_v5",
        "safe_for_policy_scoring",
        "safe_for_input",
    ] {
        assert_eq!(record[field], false, "authority must remain false: {field}");
    }
}
