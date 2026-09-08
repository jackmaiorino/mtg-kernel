use serde_json::Value;
use std::collections::{HashMap, HashSet};

const PROBE: &str =
    include_str!("../fixtures/offline_visible_card_public_reference_probe_20260810_v1.json");

#[test]
fn one_hand_public_reference_probe_is_internally_consistent() {
    let record: Value = serde_json::from_str(PROBE).unwrap();
    assert_eq!(record["schema_version"], 1);
    assert_eq!(record["observation_count"], 7);
    assert_eq!(record["human_label_agreement_count"], 7);
    assert_eq!(record["distinct_visible_card_name_count"], 5);
    assert_eq!(record["distinct_print_template_count"], 6);

    let templates = record["public_print_templates"].as_array().unwrap();
    assert_eq!(templates.len(), 6);
    let by_id: HashMap<_, _> = templates
        .iter()
        .map(|template| {
            let id = template["template_id"].as_str().unwrap();
            assert!(is_lower_sha256(
                template["public_small_image_sha256"].as_str().unwrap()
            ));
            assert!(is_uuid(template["scryfall_id"].as_str().unwrap()));
            assert!(is_uuid(template["oracle_id"].as_str().unwrap()));
            (id, template["visible_card_name"].as_str().unwrap())
        })
        .collect();
    assert_eq!(by_id.len(), templates.len());

    let observations = record["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 7);
    let mut ordinals = HashSet::new();
    let mut minimum_winner = u64::MAX;
    let mut minimum_margin = u64::MAX;
    for observation in observations {
        assert!(ordinals.insert(observation["ordinal"].as_u64().unwrap()));
        let winner = observation["winning_score_bps"].as_u64().unwrap();
        let runner_up = observation["runner_up_score_bps"].as_u64().unwrap();
        let margin = observation["margin_bps"].as_u64().unwrap();
        assert!(winner > runner_up);
        assert_eq!(margin, winner - runner_up);
        assert_eq!(
            by_id[observation["winning_template_id"].as_str().unwrap()],
            observation["human_visible_name"].as_str().unwrap()
        );
        assert!(by_id.contains_key(observation["runner_up_template_id"].as_str().unwrap()));
        minimum_winner = minimum_winner.min(winner);
        minimum_margin = minimum_margin.min(margin);
    }
    assert_eq!(ordinals, (0_u64..7).collect());
    assert_eq!(record["minimum_winning_score_bps"], minimum_winner);
    assert_eq!(record["minimum_margin_bps"], minimum_margin);
    assert!(record["nonclaim"]
        .as_str()
        .unwrap()
        .contains("not an accuracy estimate"));
}

#[test]
fn public_reference_probe_grants_no_runtime_authority() {
    let record: Value = serde_json::from_str(PROBE).unwrap();
    assert_eq!(record["source_capture"]["status"], "pending_visual_review");
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
