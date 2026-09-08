use serde_json::Value;
use std::collections::HashSet;

const CORPUS: &str =
    include_str!("../fixtures/offline_first_main_classifier_corpus_20260810_v2.json");

#[test]
fn binary_control_first_main_corpus_is_unique_and_internally_consistent() {
    let record: Value = serde_json::from_str(CORPUS).unwrap();
    assert_eq!(record["schema_version"], 2);
    assert_eq!(record["distinct_games"], 3);
    assert_eq!(record["positive_game_count"], 2);
    assert_eq!(record["sample_count"], 6);
    assert_eq!(
        record["profile_commitment_sha256"],
        "6949549bfcfa6ea2d339996d5d9b252f4a60b4c79b0a14cf55584f1efde2810f"
    );

    let samples = record["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 6);
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
        assert_eq!(values.len(), samples.len(), "duplicate field: {field}");
    }

    let mut confusion = [0_u64; 4];
    for sample in samples {
        let expected = sample["human_label"] == "turn_one_first_main";
        let observed = sample["classification"] == "match";
        match (expected, observed) {
            (true, true) => confusion[0] += 1,
            (false, false) => confusion[1] += 1,
            (false, true) => confusion[2] += 1,
            (true, false) => confusion[3] += 1,
        }
        assert_eq!(
            sample["matched_feature_count"],
            if observed { 4 } else { 2 }
        );
    }
    assert_eq!(record["confusion"]["true_positive"], confusion[0]);
    assert_eq!(record["confusion"]["true_negative"], confusion[1]);
    assert_eq!(record["confusion"]["false_positive"], confusion[2]);
    assert_eq!(record["confusion"]["false_negative"], confusion[3]);
    assert!(record["nonclaim"]
        .as_str()
        .unwrap()
        .contains("not an accuracy estimate"));
}

#[test]
fn v2_corpus_grants_no_runtime_authority() {
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
