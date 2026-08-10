use serde_json::Value;
use std::collections::HashSet;

const CORPUS: &str =
    include_str!("../fixtures/offline_mulligan_ladder_classifier_corpus_20260810_v2.json");

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[test]
fn revised_mulligan_ladder_has_a_complete_later_heldout_game() {
    let record: Value = serde_json::from_str(CORPUS).unwrap();
    assert_eq!(record["schema_version"], 2);
    assert_eq!(
        record["matcher_version"],
        "exact_binary_prompt_dark_core_bgr_sum_below_182-v3"
    );
    assert_eq!(
        record["profile_set_commitment_sha256"],
        "82f85cdc4a46008686c5336721c8794ce00479ce06910a8b032f72ac3343326f"
    );
    assert_eq!(record["development_measurement"]["exact_size_matches"], 21);
    assert_eq!(
        record["development_measurement"]["gameplay_negative_no_matches"],
        4
    );

    let samples = record["heldout_samples"].as_array().unwrap();
    assert_eq!(samples.len(), 7);
    let mut sizes = HashSet::new();
    let mut sample_ids = HashSet::new();
    let mut manifest_hashes = HashSet::new();
    let mut frame_hashes = HashSet::new();
    let mut candidate_hashes = HashSet::new();
    for sample in samples {
        assert_eq!(sample["classification"], "match");
        assert_eq!(sample["matched_profile_count"], 1);
        sizes.insert(sample["human_prospective_keep_size"].as_u64().unwrap());
        assert!(sample_ids.insert(sample["sample_id"].as_str().unwrap()));
        for (field, set) in [
            ("manifest_sha256", &mut manifest_hashes),
            ("frame_sha256", &mut frame_hashes),
            ("candidate_commitment_sha256", &mut candidate_hashes),
        ] {
            let digest = sample[field].as_str().unwrap();
            assert!(is_lower_sha256(digest));
            assert!(set.insert(digest));
        }
    }
    assert_eq!(sizes, HashSet::from([1, 2, 3, 4, 5, 6, 7]));
    assert_eq!(record["heldout_measurement"]["exact_size_matches"], 7);
    assert_eq!(record["heldout_measurement"]["wrong_size_matches"], 0);
    assert_eq!(record["heldout_measurement"]["ambiguous_results"], 0);
}

#[test]
fn revised_mulligan_ladder_grants_no_runtime_authority() {
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
