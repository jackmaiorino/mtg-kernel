use serde_json::Value;

const MEASUREMENT: &str =
    include_str!("../fixtures/offline_play_land_hand_reflow_20260810_v1.json");

#[test]
fn retained_play_land_frame_pair_has_one_well_separated_leftmost_deletion() {
    let result: Value = serde_json::from_str(MEASUREMENT).unwrap();
    assert_eq!(result["schema_version"], 1);
    assert_eq!(result["status"], "checked_untrusted_offline_measurement_only");
    assert_eq!(result["classification"], "match");
    assert_eq!(result["before_hand_count"], 8);
    assert_eq!(result["after_hand_count"], 7);
    assert_eq!(result["expected_source_ordinal"], 0);
    assert_eq!(result["unique_visual_removed_ordinal"], 0);
    assert_eq!(result["passing_deletion_candidate_count"], 1);

    let ceiling = result["maximum_match_mean_absolute_difference_milli"]
        .as_u64()
        .unwrap();
    let distances = result["matched_pair_mean_absolute_difference_milli"]
        .as_array()
        .unwrap();
    assert_eq!(distances.len(), 7);
    assert!(distances
        .iter()
        .all(|distance| distance.as_u64().is_some_and(|value| value <= ceiling)));
    assert_eq!(
        distances
            .iter()
            .map(|distance| distance.as_u64().unwrap())
            .max(),
        Some(26_989)
    );
    assert_eq!(
        result["runner_up_deletion_maximum_difference_milli"],
        66_022
    );
    assert_eq!(result["deletion_hypothesis_margin_milli"], 39_033);
    assert!(result["deletion_hypothesis_margin_milli"].as_u64().unwrap()
        >= result["minimum_deletion_hypothesis_margin_milli"]
            .as_u64()
            .unwrap());
}

#[test]
fn retained_play_land_reflow_grants_no_runtime_authority() {
    let result: Value = serde_json::from_str(MEASUREMENT).unwrap();
    for field in [
        "safe_for_semantic_evidence",
        "safe_for_observation_v5",
        "safe_for_policy_scoring",
        "safe_for_input",
    ] {
        assert_eq!(result[field], false, "authority must remain false: {field}");
    }
    assert!(result["nonclaim"]
        .as_str()
        .unwrap()
        .contains("not a held-out accuracy estimate"));
}
