use serde_json::Value;
use std::collections::HashSet;

const CORPUS: &str = include_str!("../fixtures/offline_bottom_six_reflow_corpus_20260810_v1.json");

#[test]
fn one_game_leftmost_reflow_corpus_is_internally_consistent() {
    let record: Value = serde_json::from_str(CORPUS).unwrap();
    assert_eq!(record["schema_version"], 1);
    assert_eq!(record["distinct_games"], 1);
    assert_eq!(record["positive_removed_ordinal_count"], 1);
    assert_eq!(record["transition_count"], 7);
    let threshold = record["maximum_match_mean_absolute_difference_milli"]
        .as_u64()
        .unwrap();
    assert_eq!(threshold, 25_000);

    let transitions = record["transitions"].as_array().unwrap();
    assert_eq!(transitions.len(), 7);
    for field in ["transition_id", "candidate_commitment_sha256"] {
        let values: HashSet<_> = transitions
            .iter()
            .map(|transition| transition[field].as_str().unwrap())
            .collect();
        assert_eq!(values.len(), transitions.len(), "duplicate field: {field}");
    }

    let mut confusion = [0_u64; 4];
    for transition in transitions {
        let expected = transition["human_label"] == "one_visible_card_removed";
        let observed = transition["classification"] == "match";
        match (expected, observed) {
            (true, true) => confusion[0] += 1,
            (false, false) => confusion[1] += 1,
            (false, true) => confusion[2] += 1,
            (true, false) => confusion[3] += 1,
        }
        let before = transition["before_selected_count"].as_u64().unwrap();
        let after = transition["after_selected_count"].as_u64().unwrap();
        let distances = transition["matched_pair_mean_absolute_difference_milli"]
            .as_array()
            .unwrap();
        if observed {
            assert_eq!(after, before + 1);
            assert_eq!(transition["removed_before_ordinal"], 0);
            assert_eq!(transition["passing_deletion_candidate_count"], 1);
            assert_eq!(distances.len(), usize::try_from(6 - before).unwrap());
            assert!(distances
                .iter()
                .all(|distance| distance.as_u64().is_some_and(|value| value <= threshold)));
        } else {
            assert_ne!(after, before + 1);
            assert!(transition["removed_before_ordinal"].is_null());
            assert!(distances.is_empty());
            assert_eq!(transition["passing_deletion_candidate_count"], 0);
        }
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
        .contains("leftmost visible card"));
}

#[test]
fn reflow_corpus_grants_no_runtime_authority() {
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
