use serde_json::Value;
use std::collections::BTreeSet;

const CORPUS: &str = include_str!("../fixtures/offline_bottom_six_cancel_corpus_20260810_v1.json");

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[test]
fn cancel_corpus_covers_noop_zero_and_resets_one_through_six() {
    let record: Value = serde_json::from_str(CORPUS).unwrap();
    assert_eq!(record["schema_version"], 1);
    assert!(is_lower_sha256(
        record["profile_commitment_sha256"].as_str().unwrap()
    ));
    let cases = record["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 7);
    let mut observed = BTreeSet::new();
    for case in cases {
        let prior = case["prior_selected_count"].as_u64().unwrap();
        assert!(observed.insert(prior));
        assert_eq!(case["source_visible_hand_count"], 7 - prior);
        let expected_source_actions = match prior {
            0 => 7,
            1..=5 => 8 - prior,
            _ => 2,
        };
        assert_eq!(case["source_legal_action_count"], expected_source_actions);
        assert_eq!(case["result_selected_count"], 0);
        assert_eq!(case["result_visible_hand_count"], 7);
        assert_eq!(case["result_legal_action_count"], 7);
        assert!(
            case["result_captured_at_unix_millis"].as_u64().unwrap()
                > case["source_captured_at_unix_millis"].as_u64().unwrap()
        );
        assert_eq!(
            case["transition_kind"],
            if prior == 0 {
                "visible_no_op"
            } else {
                "reset_all_selections"
            }
        );
        assert_eq!(
            case["control_layout"],
            if prior == 6 {
                "done_and_cancel"
            } else {
                "cancel_only"
            }
        );
        for field in [
            "source_manifest_sha256",
            "source_frame_sha256",
            "source_candidate_commitment_sha256",
            "result_manifest_sha256",
            "result_frame_sha256",
            "result_candidate_commitment_sha256",
        ] {
            assert!(is_lower_sha256(case[field].as_str().unwrap()), "{field}");
        }
        assert_ne!(
            case["source_manifest_sha256"],
            case["result_manifest_sha256"]
        );
        assert_ne!(case["source_frame_sha256"], case["result_frame_sha256"]);
        assert_ne!(
            case["source_candidate_commitment_sha256"],
            case["result_candidate_commitment_sha256"]
        );
    }
    assert_eq!(observed, (0_u64..=6).collect());
}

#[test]
fn cancel_corpus_grants_no_runtime_or_competitive_authority() {
    let record: Value = serde_json::from_str(CORPUS).unwrap();
    assert_eq!(record["supervised_visible_clicks_only"], true);
    for field in [
        "purchase_or_trade_performed",
        "queue_or_event_entry_performed",
        "league_or_challenge_input_performed",
        "safe_for_live_frame",
        "safe_for_semantic_evidence",
        "safe_for_observation_v5",
        "safe_for_policy_scoring",
        "safe_for_input",
    ] {
        assert_eq!(record[field], false, "authority must remain false: {field}");
    }
}
