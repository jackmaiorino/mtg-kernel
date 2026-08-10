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
        "pub fn confirm_pregame_keep_to_bottom_six_transition_v3(\n    plan: OpaqueMtgoPregameActionPlanV3,\n    after: OpaqueMtgoDxgiBottomSixInitialMeasurementV3,"
    ));
    assert!(source.contains(
        "pub fn measure_mtgo_dxgi_first_main_candidate_v3(\n    source_frame: OpaqueMtgoDxgiFrameCandidateV3,"
    ));
    assert!(source.contains(
        "pub fn measure_mtgo_dxgi_bottom_six_initial_candidate_v3(\n    source_frame: OpaqueMtgoDxgiFrameCandidateV3,"
    ));
    assert!(source.contains(
        "pub fn measure_mtgo_dxgi_bottom_six_state_candidate_v3(\n    source_frame: OpaqueMtgoDxgiFrameCandidateV3,"
    ));
    assert!(source.contains("pub struct OpaqueMtgoDxgiBottomSixStateMeasurementV3 {"));
    assert!(source.contains("measurement: CheckedUntrustedMtgoOfflineBottomSixStateCandidateV1,"));
    assert!(source.contains("fn build_pregame_action_plan_parts_v3("));
    assert!(source.contains("fn validate_mulligan_postcondition_parts_v3("));
    assert!(source.contains("fn validate_keep_first_main_postcondition_parts_v3("));
    assert!(source.contains("fn validate_keep_bottom_six_postcondition_parts_v3("));
    assert!(!source.contains("pub fn build_pregame_action_plan_parts_v3("));
    assert!(!source.contains("pub fn validate_mulligan_postcondition_parts_v3("));
    assert!(!source.contains("pub fn validate_keep_first_main_postcondition_parts_v3("));
    assert!(!source.contains("pub fn validate_keep_bottom_six_postcondition_parts_v3("));
    assert!(!source.contains("pub fn target_point_client_px_v3"));
    assert!(!source.contains("safe_for_live_input_v3(&self) -> bool {\n        true"));
}

#[test]
fn live_actuator_is_isolated_authorization_bound_and_postcondition_locked() {
    let source = include_str!("../src/actuator.rs");
    for required in [
        "RATIFIED_PRIVATE_MATCH_AUTHORIZATION_COMMITMENT_V3: Option<&str> = None",
        "ratify_private_match_authorization_v3",
        "validate_authorization_for_mode_v1",
        "MtgoRuntimeModeV1::PrivateMatchInput",
        "prepare_pregame_actuation_v3",
        "SendInput(&inputs",
        "WindowFromPoint",
        "AwaitingVisiblePostcondition",
        "confirm_pending_pregame_mulligan_v3",
        "confirm_pending_pregame_keep_to_bottom_six_v3",
        "confirm_pending_pregame_keep_to_first_main_v3",
    ] {
        assert!(
            source.contains(required),
            "live actuator is missing required guard: {required}"
        );
    }
    for forbidden in [
        "INPUT_KEYBOARD",
        "KEYBDINPUT",
        "keybd_event",
        "mouse_event",
        "PostMessage",
        "SendMessage",
        "ReadProcessMemory",
        "WriteProcessMemory",
        "CreateRemoteThread",
        "UIAutomation",
        "WinHttp",
        "WinSock",
        "safe_for_next_input_v3(&self) -> bool {\n        true",
    ] {
        assert!(
            !source.contains(forbidden),
            "live actuator contains a forbidden API or authority path: {forbidden}"
        );
    }
    assert!(source.contains(
        "pub fn execute_authorized_private_match_pregame_action_v3(\n    plan: OpaqueMtgoPregameActionPlanV3,\n    authorization: RatifiedMtgoPrivateMatchAuthorizationV3,"
    ));
    assert!(!source.contains("pub fn target_x_desktop_px"));
    assert!(!source.contains("pub fn target_y_desktop_px"));
}
