use mtgo_blackbox_v1::{
    validate_pregame_calibration_trace_v1, MtgoPregameActionSemanticV1,
    MtgoPregameCalibrationTraceV1,
};

fn fixture() -> MtgoPregameCalibrationTraceV1 {
    serde_json::from_str(include_str!(
        "../fixtures/solitaire_keep_transition_v1.json"
    ))
    .expect("checked-in trace fixture must parse")
}

#[test]
fn live_solitaire_keep_transition_is_structurally_checked_only() {
    let checked = validate_pregame_calibration_trace_v1(fixture()).unwrap();
    assert_eq!(
        checked.action(),
        &MtgoPregameActionSemanticV1::KeepOpeningHand
    );
    assert_eq!(
        checked.before_frame_sha256(),
        "775c348e5393d0c77ca839c5abad18cd672282c230fd40712fdd9f8015abf0cf"
    );
    assert_eq!(
        checked.after_frame_sha256(),
        "1258c6810a040b7c4a99cdac27ac8b6c5e1b28373cf05bf16e0c9ada7db40fa9"
    );
    assert_eq!(checked.transition_commitment_sha256().len(), 64);
}

#[test]
fn unchanged_or_missing_visible_postconditions_fail_closed() {
    let mut unchanged = fixture();
    unchanged.visible_postconditions[0].after_bgra8_sha256 = unchanged.visible_postconditions[0]
        .before_bgra8_sha256
        .clone();
    assert_eq!(
        validate_pregame_calibration_trace_v1(unchanged)
            .err()
            .expect("unchanged postcondition must fail")
            .code(),
        "pregame_trace_postcondition_unchanged"
    );

    let mut missing = fixture();
    missing.visible_postconditions.retain(|item| {
        item.change != mtgo_blackbox_v1::MtgoPregameVisibleChangeV1::PhaseBarChanged
    });
    assert_eq!(
        validate_pregame_calibration_trace_v1(missing)
            .err()
            .expect("missing required postcondition must fail")
            .code(),
        "pregame_trace_required_postcondition_missing"
    );
}

#[test]
fn source_order_geometry_and_authority_claims_fail_closed() {
    let mut order = fixture();
    order.after_frame.sequence = order.before_frame.sequence;
    assert_eq!(
        validate_pregame_calibration_trace_v1(order)
            .err()
            .expect("frame order must fail")
            .code(),
        "pregame_trace_frame_order_invalid"
    );

    let mut geometry = fixture();
    geometry.action_control_before.rect_client_px.x = 1_549;
    geometry.action_control_before.rect_client_px.width = 2;
    assert_eq!(
        validate_pregame_calibration_trace_v1(geometry)
            .err()
            .expect("out-of-bounds action region must fail")
            .code(),
        "pregame_trace_action_control_invalid"
    );

    let mut authority = fixture();
    authority.after_frame.safe_for_input = true;
    assert_eq!(
        validate_pregame_calibration_trace_v1(authority)
            .err()
            .expect("preview authority claim must fail")
            .code(),
        "pregame_trace_preview_claims_authority"
    );
}

#[test]
fn strict_json_rejects_unknown_fields() {
    let source = include_str!("../fixtures/solitaire_keep_transition_v1.json");
    let altered = source.replacen(
        "\"trace_id\":",
        "\"process_memory\": \"forbidden\", \"trace_id\":",
        1,
    );
    assert!(serde_json::from_str::<MtgoPregameCalibrationTraceV1>(&altered).is_err());
}
