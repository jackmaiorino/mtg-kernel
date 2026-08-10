use serde_json::Value;
use std::collections::{HashMap, HashSet};

const CORPUS: &str =
    include_str!("../fixtures/offline_mulligan_ladder_classifier_corpus_20260810_v1.json");

#[test]
fn mulligan_ladder_corpus_is_unique_and_internally_consistent() {
    let record: Value = serde_json::from_str(CORPUS).unwrap();
    assert_eq!(record["schema_version"], 1);
    assert_eq!(record["measurement"]["distinct_games"], 2);
    assert_eq!(record["measurement"]["sample_count"], 10);
    assert_eq!(
        record["profile_set_commitment_sha256"],
        "bc278fde2cf9e5999bfc8d3dbbf437619ef8974d014b3dff2d4be14b58b28485"
    );

    let samples = record["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 10);
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

    let mut exact_size_matches = 0_u64;
    let mut wrong_size_matches = 0_u64;
    let mut ambiguous_results = 0_u64;
    let mut negative_no_matches = 0_u64;
    let mut observed_sizes = HashMap::new();
    for sample in samples {
        let human = sample["human_prospective_keep_size"].as_u64();
        let observed = sample["observed_prospective_keep_size"].as_u64();
        match sample["classification"].as_str().unwrap() {
            "match" => {
                if human == observed {
                    exact_size_matches += 1;
                } else {
                    wrong_size_matches += 1;
                }
                *observed_sizes.entry(observed.unwrap()).or_insert(0_u64) += 1;
                assert_eq!(sample["matched_profile_count"], 1);
            }
            "no_match" => {
                assert_eq!(human, None);
                assert_eq!(observed, None);
                assert_eq!(sample["matched_profile_count"], 0);
                negative_no_matches += 1;
            }
            "ambiguous" => ambiguous_results += 1,
            other => panic!("unknown classification: {other}"),
        }
    }
    assert_eq!(
        observed_sizes.keys().copied().collect::<HashSet<_>>(),
        HashSet::from([1, 2, 3, 4, 5, 6, 7])
    );
    assert_eq!(
        record["measurement"]["exact_size_matches"],
        exact_size_matches
    );
    assert_eq!(
        record["measurement"]["wrong_size_matches"],
        wrong_size_matches
    );
    assert_eq!(
        record["measurement"]["ambiguous_results"],
        ambiguous_results
    );
    assert_eq!(
        record["measurement"]["gameplay_negative_no_matches"],
        negative_no_matches
    );
}

#[test]
fn mulligan_ladder_corpus_grants_no_runtime_authority_and_states_nonclaim() {
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
        assert_eq!(
            record[field], false,
            "corpus authority must remain false: {field}"
        );
    }
}
