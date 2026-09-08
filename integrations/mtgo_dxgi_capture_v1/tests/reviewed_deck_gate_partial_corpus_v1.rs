use mtgo_dxgi_capture_v1::{
    check_built_in_reviewed_deck_gate_partial_corpus_v1,
    check_untrusted_reviewed_deck_gate_partial_corpus_v1,
    MTGO_REVIEWED_DECK_GATE_PARTIAL_CORPUS_SHA256_V1,
};
use serde_json::Value;
use std::process::Command;

const RECEIPT_JSON: &str =
    include_str!("../assets/reviewed_modern_deck_gate_partial_corpus_v1.json");

#[test]
fn reviewed_partial_corpus_qualifies_two_states_per_mode_without_authority() {
    let report = check_built_in_reviewed_deck_gate_partial_corpus_v1().unwrap();
    assert_eq!(
        report.receipt_sha256,
        MTGO_REVIEWED_DECK_GATE_PARTIAL_CORPUS_SHA256_V1
    );
    assert_eq!(report.reviewed_source_count, 5);
    assert_eq!(report.partial_profile_count, 4);
    assert_eq!(report.league_safe_state_count, 2);
    assert_eq!(report.challenge_safe_state_count, 2);
    assert!(!report.open_entry_review_state_present);
    assert!(!report.passes_three_state_gate);
    assert!(report.safe_for_offline_partial_classifier_reference_generation);
    assert!(!report.safe_for_live_classification);
    assert!(!report.grants_semantic_evidence);
    assert!(!report.grants_policy_scoring);
    assert!(!report.grants_input);
    assert!(!report.grants_open_entry_review);
    assert!(!report.grants_event_entry);
    assert!(!report.grants_spending);
}

#[test]
fn partial_corpus_rejects_profile_loss_and_authority_promotion() {
    let mut missing_profile: Value = serde_json::from_str(RECEIPT_JSON).unwrap();
    missing_profile["partial_reference_profiles"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(check_untrusted_reviewed_deck_gate_partial_corpus_v1(
        &serde_json::to_vec(&missing_profile).unwrap()
    )
    .is_err());

    let mut promoted: Value = serde_json::from_str(RECEIPT_JSON).unwrap();
    promoted["authority"]["grants_live_classification"] = Value::Bool(true);
    assert!(check_untrusted_reviewed_deck_gate_partial_corpus_v1(
        &serde_json::to_vec(&promoted).unwrap()
    )
    .is_err());

    let mut invented_third_state: Value = serde_json::from_str(RECEIPT_JSON).unwrap();
    invented_third_state["passes_three_state_gate"] = Value::Bool(true);
    assert!(check_untrusted_reviewed_deck_gate_partial_corpus_v1(
        &serde_json::to_vec(&invented_third_state).unwrap()
    )
    .is_err());

    let mut substituted_reference: Value = serde_json::from_str(RECEIPT_JSON).unwrap();
    substituted_reference["reference_hashes"]["selected_deck_label_region_sha256"] =
        Value::String("0".repeat(64));
    assert!(check_untrusted_reviewed_deck_gate_partial_corpus_v1(
        &serde_json::to_vec(&substituted_reference).unwrap()
    )
    .is_err());

    let mut unknown_field: Value = serde_json::from_str(RECEIPT_JSON).unwrap();
    unknown_field["grants_entry_by_implication"] = Value::Bool(true);
    assert!(check_untrusted_reviewed_deck_gate_partial_corpus_v1(
        &serde_json::to_vec(&unknown_field).unwrap()
    )
    .is_err());
}

#[test]
fn qualification_executable_reports_the_same_non_authorizing_receipt() {
    let output = Command::new(env!(
        "CARGO_BIN_EXE_check_mtgo_reviewed_deck_gate_partial_corpus_v1"
    ))
    .output()
    .unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["receipt_sha256"],
        MTGO_REVIEWED_DECK_GATE_PARTIAL_CORPUS_SHA256_V1
    );
    assert_eq!(report["league_safe_state_count"], 2);
    assert_eq!(report["challenge_safe_state_count"], 2);
    assert_eq!(report["passes_three_state_gate"], false);
    assert_eq!(report["safe_for_live_classification"], false);
    assert_eq!(report["grants_input"], false);
    assert_eq!(report["grants_event_entry"], false);
    assert_eq!(report["grants_spending"], false);
}
