#[test]
fn production_source_uses_composed_desktop_and_excludes_hidden_or_input_apis() {
    let source = include_str!("../src/probe.rs");
    for forbidden in [
        "PrintWindow",
        "WM_PRINT",
        "GetWindowDC",
        "BitBlt",
        "DwmRegisterThumbnail",
        "Windows.Graphics.Capture",
        "ReadProcessMemory",
        "WriteProcessMemory",
        "SetWindowsHookEx",
        "CreateRemoteThread",
        "SendInput",
        "mouse_event",
        "keybd_event",
        "PostMessage",
        "SendMessage",
        "UIAutomation",
        "WinHttp",
        "WinSock",
        "pcap",
    ] {
        assert!(
            !source.contains(forbidden),
            "production source contains forbidden API or channel: {forbidden}"
        );
    }
    for required in [
        "DuplicateOutput",
        "AcquireNextFrame",
        "ProtectedContentMaskedOut",
        "WinVerifyTrust",
        "DwmGetWindowAttribute",
        "GetForegroundWindow",
    ] {
        assert!(
            source.contains(required),
            "production source is missing required admission operation: {required}"
        );
    }
}

#[test]
fn public_measurement_consumes_only_the_opaque_frame_and_keeps_parts_private() {
    let source = include_str!("../src/probe.rs");
    assert!(source.contains(
        "pub fn measure_mtgo_dxgi_mulligan_ladder_candidate_v3(\n    source_frame: OpaqueMtgoDxgiFrameCandidateV3,"
    ));
    assert!(source.contains("fn measure_mulligan_ladder_parts_v3("));
    assert!(!source.contains("pub fn measure_mulligan_ladder_parts_v3("));
    for forbidden in [
        "canonical_pixels_v3",
        "to_observation_v5",
        "to_action_intent",
        "send_input",
    ] {
        assert!(
            !source.contains(forbidden),
            "opaque capture seam exposes forbidden downstream conversion: {forbidden}"
        );
    }
}

#[test]
fn pregame_scoring_consumes_opaque_measurement_and_cannot_mint_input() {
    let source = include_str!("../src/probe.rs");
    assert!(source.contains(
        "pub fn score_and_select_pregame_model_v3<S: MtgoExternalPregameScorerV3>(\n    measurement: OpaqueMtgoDxgiMulliganMeasurementV3,"
    ));
    assert!(source.contains(
        "pub fn validate_pregame_score_response_v3(\n    measurement: OpaqueMtgoDxgiMulliganMeasurementV3,"
    ));
    assert!(source.contains("fn validate_pregame_score_response_parts_v3("));
    assert!(!source.contains("pub fn validate_pregame_score_response_parts_v3("));
    assert!(!source.contains("pub fn make_live_input"));
}

#[test]
fn pregame_action_plan_keeps_coordinates_private_and_confirmation_capture_bound() {
    let source = include_str!("../src/probe.rs");
    assert!(source.contains(
        "pub fn build_pregame_action_plan_v3(\n    selection: OpaqueMtgoPregameModelSelectionV3,"
    ));
    assert!(source.contains(
        "pub fn confirm_pregame_mulligan_transition_v3(\n    plan: OpaqueMtgoPregameActionPlanV3,\n    after: OpaqueMtgoDxgiMulliganMeasurementV3,"
    ));
    assert!(source.contains(
        "pub fn confirm_pregame_keep_to_first_main_transition_v3(\n    plan: OpaqueMtgoPregameActionPlanV3,\n    after: OpaqueMtgoDxgiFirstMainMeasurementV3,"
    ));
    assert!(source.contains(
        "pub fn measure_mtgo_dxgi_first_main_candidate_v3(\n    source_frame: OpaqueMtgoDxgiFrameCandidateV3,"
    ));
    assert!(source.contains("fn build_pregame_action_plan_parts_v3("));
    assert!(source.contains("fn validate_mulligan_postcondition_parts_v3("));
    assert!(source.contains("fn validate_keep_first_main_postcondition_parts_v3("));
    assert!(!source.contains("pub fn build_pregame_action_plan_parts_v3("));
    assert!(!source.contains("pub fn validate_mulligan_postcondition_parts_v3("));
    assert!(!source.contains("pub fn validate_keep_first_main_postcondition_parts_v3("));
    assert!(!source.contains("pub fn target_point_client_px_v3"));
    assert!(!source.contains("safe_for_live_input_v3(&self) -> bool {\n        true"));
}
