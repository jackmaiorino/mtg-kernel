use serde_json::Value;
use std::collections::HashSet;

const CORPUS: &str =
    include_str!("../fixtures/offline_bottom_six_initial_classifier_corpus_20260810_v1.json");

#[test]
fn one_positive_bottom_six_initial_corpus_is_unique_and_consistent() {
    let record: Value = serde_json::from_str(CORPUS).unwrap();
    assert_eq!(record["schema_version"], 1);
    assert_eq!(record["distinct_games"], 2);
    assert_eq!(record["positive_game_count"], 1);
    assert_eq!(record["sample_count"], 5);
    assert_eq!(
        record["profile_commitment_sha256"],
        "caafef8397e55e58f97ae24bca403390cb0d86a7ac2dd2fa23f30021f95479a8"
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

    let mut confusion = [0_u64; 4];
    for sample in samples {
        let expected = sample["human_label"] == "bottom_six_zero_selected";
        let observed = sample["classification"] == "match";
        match (expected, observed) {
            (true, true) => confusion[0] += 1,
            (false, false) => confusion[1] += 1,
            (false, true) => confusion[2] += 1,
            (true, false) => confusion[3] += 1,
        }
        assert!(matches!(
            sample["matched_exact_region_count"].as_u64(),
            Some(0..=3)
        ));
        assert!(matches!(
            sample["bright_occupancy_pixel_count"].as_u64(),
            Some(0..=900)
        ));
        assert_eq!(
            sample["occupancy_matches"],
            sample["bright_occupancy_pixel_count"].as_u64().unwrap() >= 200
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
    assert!(record["nonclaim"]
        .as_str()
        .unwrap()
        .contains("another required bottom count"));
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
