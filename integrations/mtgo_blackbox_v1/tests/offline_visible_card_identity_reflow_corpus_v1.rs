use serde_json::Value;
use std::collections::{HashMap, HashSet};

const CORPUS: &str =
    include_str!("../fixtures/offline_visible_card_identity_reflow_corpus_20260810_v1.json");

#[test]
fn one_game_visible_card_identity_reflow_corpus_is_internally_consistent() {
    let record: Value = serde_json::from_str(CORPUS).unwrap();
    assert_eq!(record["schema_version"], 1);
    assert_eq!(record["profile_scope"], "one_observed_bottom_six_hand_only");
    assert_eq!(
        record["profile_status"],
        "checked_untrusted_calibration_not_ratified"
    );
    assert_eq!(record["source_deck_visible_card_count"], 280);
    assert_eq!(record["source_deck_profile_complete"], false);
    assert_eq!(record["state_count"], 7);
    assert_eq!(record["distinct_games"], 1);
    assert_eq!(record["positive_removed_ordinal_count"], 1);

    for field in [
        "profile_file_sha256",
        "profile_commitment_sha256",
        "observed_hand_manifest_sha256",
    ] {
        assert!(is_lower_sha256(record[field].as_str().unwrap()), "{field}");
    }

    let templates = record["templates"].as_array().unwrap();
    assert_eq!(templates.len(), 7);
    let template_ids: HashSet<_> = templates
        .iter()
        .map(|template| template["template_id"].as_str().unwrap())
        .collect();
    assert_eq!(template_ids.len(), templates.len());
    let oracle_by_name: HashMap<_, _> = templates
        .iter()
        .map(|template| {
            assert!(is_uuid(template["public_print_id"].as_str().unwrap()));
            assert!(is_uuid(template["public_oracle_id"].as_str().unwrap()));
            assert!(is_lower_sha256(
                template["public_reference_image_sha256"].as_str().unwrap()
            ));
            assert!(is_lower_sha256(
                template["reference_bgr8_sha256"].as_str().unwrap()
            ));
            (
                template["visible_card_name"].as_str().unwrap(),
                template["public_oracle_id"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(oracle_by_name.len(), 4);

    let distance_ceiling = record["maximum_mean_absolute_difference_milli"]
        .as_u64()
        .unwrap();
    let margin_floor = record["minimum_distinct_name_margin_milli"]
        .as_u64()
        .unwrap();
    let states = record["states"].as_array().unwrap();
    assert_eq!(states.len(), 7);
    let mut prior_names: Option<Vec<&str>> = None;
    let mut maximum_distance = 0_u64;
    let mut minimum_margin = u64::MAX;
    let mut commitments = HashSet::new();
    for (selected_count, state) in states.iter().enumerate() {
        assert_eq!(
            state["selected_count"],
            u64::try_from(selected_count).unwrap()
        );
        assert_eq!(state["classification"], "match");
        assert!(is_lower_sha256(
            state["source_manifest_sha256"].as_str().unwrap()
        ));
        assert!(is_lower_sha256(
            state["source_stage_commitment_sha256"].as_str().unwrap()
        ));
        assert!(commitments.insert(state["candidate_commitment_sha256"].as_str().unwrap()));
        let names: Vec<_> = state["visible_card_names"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        assert_eq!(names.len(), 7 - selected_count);
        if let Some(prior) = prior_names {
            assert_eq!(names, prior[1..]);
        }
        prior_names = Some(names);
        let distances = state["mean_absolute_difference_milli"].as_array().unwrap();
        let margins = state["distinct_name_margin_milli"].as_array().unwrap();
        assert_eq!(distances.len(), 7 - selected_count);
        assert_eq!(margins.len(), distances.len());
        for distance in distances {
            let value = distance.as_u64().unwrap();
            assert!(value <= distance_ceiling);
            maximum_distance = maximum_distance.max(value);
        }
        for margin in margins {
            let value = margin.as_u64().unwrap();
            assert!(value >= margin_floor);
            minimum_margin = minimum_margin.min(value);
        }
    }
    assert_eq!(
        record["maximum_observed_mean_absolute_difference_milli"],
        maximum_distance
    );
    assert_eq!(
        record["minimum_observed_distinct_name_margin_milli"],
        minimum_margin
    );
    assert!(record["nonclaim"]
        .as_str()
        .unwrap()
        .contains("not an accuracy estimate"));
    assert!(record["nonclaim"]
        .as_str()
        .unwrap()
        .contains("not a complete 280-card deck profile"));
}

#[test]
fn visible_card_identity_reflow_corpus_grants_no_runtime_authority() {
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

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}
