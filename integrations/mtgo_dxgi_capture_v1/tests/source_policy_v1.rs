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
fn acting_player_duel_mode_is_role_explicit_and_still_non_actionable() {
    let library = include_str!("../src/lib.rs");
    let probe = include_str!("../src/probe.rs");
    for required in [
        "CaptureWindowModeV2::DuelGame",
        "\"duel_game\"",
        "\"acting_player_duel\"",
        "safe_for_semantic_evidence: false",
        "safe_for_ocr: false",
        "safe_for_policy_scoring: false",
        "safe_for_input: false",
    ] {
        assert!(
            library.contains(required) || probe.contains(required),
            "acting-player duel boundary is missing: {required}"
        );
    }
}

#[test]
fn admitted_duel_frame_requires_an_opaque_profile_and_retains_no_downstream_authority() {
    let source = include_str!("../src/probe/duel_profile_frame.rs");
    for required in [
        "capture_admitted_mtgo_duel_visible_frame_v1",
        "AdmittedMtgoDuelPerceptionProfileV1",
        "OpaqueMtgoAdmittedDuelVisibleFrameV1",
        "CaptureWindowModeV2::DuelGame",
        "profile.executable_sha256()",
        "profile.signer_thumbprint()",
        "profile.signer_subject_sha256()",
        "profile.dpi()",
        "profile.client_size_px()",
        "profile.output_identity_sha256()",
        "profile.game_format()",
        "perception_profile_admission_commitment_sha256",
        "frame_profile_binding_sha256",
    ] {
        assert!(
            source.contains(required),
            "admitted duel-frame seam is missing: {required}"
        );
    }
    for forbidden in [
        "pub fn bind_captured_duel_frame_to_profile_v1",
        "pub fn canonical_bgra8",
        "pub fn preview_png",
        "to_observation",
        "to_model_request",
        "to_action_intent",
        "target_point_client",
        "safe_for_semantic_evidence_v1(&self) -> bool {\n        true",
        "safe_for_model_scoring_v1(&self) -> bool {\n        true",
        "safe_for_input_v1(&self) -> bool {\n        true",
        "SendInput",
        "ReadProcessMemory",
        "WriteProcessMemory",
    ] {
        assert!(
            !source.contains(forbidden),
            "admitted duel-frame seam exposes forbidden authority: {forbidden}"
        );
    }
}

#[test]
fn competitive_navigation_frame_requires_admitted_profile_and_exact_account_title() {
    let source = include_str!("../src/probe/competitive_navigation_profile_frame.rs");
    for required in [
        "capture_admitted_mtgo_competitive_navigation_frame_v1",
        "AdmittedMtgoCompetitiveNavigationProfileV1",
        "OpaqueMtgoAdmittedCompetitiveNavigationFrameV1",
        "CaptureWindowModeV2::MainClient",
        "runtime.executable_sha256()",
        "runtime.signer_thumbprint()",
        "runtime.signer_subject_sha256()",
        "runtime.window_title_sha256()",
        "runtime.approved_account_alias_sha256()",
        "runtime.account_identity_rect_client_px()",
        "runtime.account_identity_region_sha256()",
        "visible_frame_region_content_sha256_v1",
        "runtime.dpi()",
        "runtime.client_size_px()",
        "runtime.output_identity_sha256()",
        "profile_admission_commitment_sha256",
        "frame_profile_binding_sha256",
        "opaque_main_client_navigation_pixels_no_classification_no_entry_no_spending_no_input",
    ] {
        assert!(
            source.contains(required),
            "competitive navigation frame seam is missing: {required}"
        );
    }
    for forbidden in [
        "pub fn bind_captured_competitive_navigation_frame_to_profile_v1",
        "pub fn canonical_bgra8",
        "pub fn preview_png",
        "to_lifecycle",
        "join_control",
        "target_point_client",
        "safe_for_lifecycle_classification_v1(&self) -> bool {\n        true",
        "permits_event_entry_v1(&self) -> bool {\n        true",
        "permits_spending_v1(&self) -> bool {\n        true",
        "safe_for_input_v1(&self) -> bool {\n        true",
        "SendInput",
        "ReadProcessMemory",
        "WriteProcessMemory",
    ] {
        assert!(
            !source.contains(forbidden),
            "competitive navigation frame seam exposes forbidden authority: {forbidden}"
        );
    }
}

#[test]
fn competitive_navigation_classifier_is_exact_bounded_and_non_actionable() {
    let source = include_str!("../src/probe/competitive_navigation_runtime.rs");
    for required in [
        "verify_competitive_navigation_classifier_runtime_v1",
        "classify_admitted_mtgo_competitive_navigation_frame_v1",
        "AdmittedMtgoCompetitiveNavigationProfileV1",
        "OpaqueMtgoAdmittedCompetitiveNavigationFrameV1",
        "OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1",
        "OpaqueMtgoClassifiedCompetitiveNavigationFrameV1",
        "classifier_assets_manifest_bytes",
        "MTGO_VISIBLE_COMPETITIVE_NAVIGATION_V1",
        "env_clear()",
        "Stdio::piped()",
        "child.kill()",
        "MAX_CLASSIFIER_RESPONSE_BYTES_V1",
        "check_untrusted_competitive_navigation_prediction_v1",
        "rehash_lifecycle_visible_facts_v1",
        "opaque_four_slice_classification_no_entry_no_spending_no_input",
        "safe_for_lifecycle_classification_v1(&self) -> bool {\n        false",
    ] {
        assert!(
            source.contains(required),
            "competitive navigation classifier seam is missing: {required}"
        );
    }
    for forbidden in [
        "cmd.exe",
        "powershell",
        "pub fn canonical_bgra8",
        "pub fn visible_fact_rectangles",
        "pub fn join_control",
        "pub fn input_command",
        "safe_for_lifecycle_classification_v1(&self) -> bool {\n        true",
        "permits_event_entry_v1(&self) -> bool {\n        true",
        "permits_spending_v1(&self) -> bool {\n        true",
        "safe_for_input_v1(&self) -> bool {\n        true",
        "SendInput",
        "ReadProcessMemory",
        "WriteProcessMemory",
    ] {
        assert!(
            !source.contains(forbidden),
            "competitive navigation classifier seam exposes forbidden authority: {forbidden}"
        );
    }
}

#[test]
fn opaque_duel_perception_runtime_retains_pixels_through_scoring_without_input_authority() {
    let source = include_str!("../src/probe/duel_perception_runtime.rs");
    for required in [
        "verify_duel_perception_runtime_v1",
        "perceive_admitted_duel_frame_v1",
        "score_and_select_opaque_admitted_duel_perception_v1",
        "resolve_opaque_profile_bound_duel_control_v1",
        "prepare_opaque_competitive_duel_action_plan_v1",
        "prepare_opaque_competitive_duel_pass_actuation_v1",
        "check_untrusted_competitive_gameplay_before_input_pixels_v1",
        "inspect_untrusted_competitive_gameplay_postcondition_candidate_pixels_v1",
        "confirm_opaque_competitive_duel_pass_postcondition_v1",
        "bind_opaque_duel_control_to_competitive_action_plan_v1",
        "OpaqueMtgoVerifiedDuelPerceptionRuntimeV1",
        "OpaqueMtgoAdmittedDuelPerceptionV1",
        "OpaqueMtgoProfileBoundDuelModelSelectionV1",
        "OpaqueMtgoProfileBoundDuelResolvedControlV1",
        "OpaqueMtgoCompetitiveDuelActionPlanV1",
        "OpaqueMtgoPreparedCompetitiveDuelPassV1",
        "MtgoDuelPerceptionProcessResponseV1",
        "MtgoDuelPerceptionRequestHeaderV1",
        "check_untrusted_duel_perception_request_v1",
        "CheckedUntrustedMtgoDuelPerceptionRequestV1",
        "request_commitment_sha256",
        "reconstruction_audit: MtgoObservationReconstructionAuditV1",
        "visible_controls: MtgoVisibleActionControlSetV1",
        "visible_frame_region_content_sha256_v1",
        "validate_observed_decision_v1",
        "check_untrusted_dxgi_capture_artifact_v1",
        "validate_dxgi_bound_observation_reconstruction_audit_v1",
        "check_untrusted_dxgi_observed_decision_candidate_v1",
        "score_and_select_profile_bound_duel_candidate_v1",
        "resolve_profile_bound_selected_visible_control_v1",
        "prepare_profile_bound_action_postcondition_plan_v1",
        "bind_profile_bound_action_plan_to_competitive_match_v1",
        "duel_action_family_v1",
        "profile.supported_action_families()",
        "source_frame: OpaqueMtgoAdmittedDuelVisibleFrameV1",
        "perception: OpaqueMtgoAdmittedDuelPerceptionV1",
        "env_clear()",
        "MAX_PERCEPTION_RESPONSE_BYTES_V1",
        "runtime timed out",
        "safe_for_input_v1(&self) -> bool",
        "permits_event_entry_v1(&self) -> bool",
    ] {
        assert!(
            source.contains(required),
            "opaque duel-perception runtime is missing: {required}"
        );
    }
    for forbidden in [
        "pub fn canonical_bgra8",
        "pub fn observation",
        "pub fn legal_actions",
        "pub fn selected_semantic",
        "pub fn rect_client_px",
        "pub fn target_point_client_px",
        "safe_for_input_v1(&self) -> bool {\n        true",
        "permits_event_entry_v1(&self) -> bool {\n        true",
        "ReadProcessMemory",
        "WriteProcessMemory",
        "CreateRemoteThread",
        "UIAutomation",
        "WinHttp",
        "WinSock",
        "SendInput",
    ] {
        assert!(
            !source.contains(forbidden),
            "opaque duel-perception runtime exposes a forbidden channel: {forbidden}"
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
fn pinned_live_frame_binds_source_identity_and_layout_without_downstream_authority() {
    let source = include_str!("../src/probe/live_frame.rs");
    for required in [
        "capture_pinned_current_solitaire_visible_frame_v1",
        "OpaqueMtgoPinnedSolitaireVisibleFrameV1",
        "measure_pinned_current_solitaire_first_main_v1",
        "OpaqueMtgoPinnedSolitaireFirstMainMeasurementV1",
        "measure_pinned_current_solitaire_first_main_visible_hand_v1",
        "OpaqueMtgoPinnedSolitaireFirstMainVisibleHandV1",
        "measure_pinned_current_solitaire_mulligan_ladder_v1",
        "OpaqueMtgoPinnedSolitaireMulliganMeasurementV1",
        "measure_pinned_current_solitaire_mulligan_visible_hand_v1",
        "OpaqueMtgoPinnedSolitaireMulliganVisibleHandV1",
        "score_and_select_pinned_current_solitaire_pregame_v1",
        "OpaqueMtgoPinnedSolitairePregameSelectionV1",
        "build_pinned_current_solitaire_pregame_action_plan_v1",
        "OpaqueMtgoPinnedSolitairePregameActionPlanV1",
        "PINNED_EXECUTABLE_SHA256_V1",
        "PINNED_SIGNER_THUMBPRINT_V1",
        "PINNED_SIGNER_SUBJECT_SHA256_V1",
        "PINNED_VISIBLE_TITLE_V1",
        "PINNED_OUTPUT_DEVICE_V1",
        "PINNED_SOLITAIRE_PROFILE_COMMITMENT_V1",
        "CaptureWindowModeV2::SolitaireGame",
        "expected_game_format: Some(\"Freeform\".to_owned())",
        "capture_commitment_v3(",
        "into_checked_untrusted_perception_candidate_v1",
    ] {
        assert!(
            source.contains(required),
            "pinned visible-frame boundary is missing: {required}"
        );
    }
    for forbidden in [
        "unsafe {",
        "pub fn bind_pinned_current_solitaire_visible_frame_v1",
        "pub fn canonical_bgra8_v1",
        "pub fn preview_png_v1",
        "safe_for_semantic_evidence_v1(&self) -> bool {\n        true",
        "safe_for_observation_v5_v1(&self) -> bool {\n        true",
        "safe_for_policy_scoring_v1(&self) -> bool {\n        true",
        "safe_for_input_v1(&self) -> bool {\n        true",
        "into_checked_untrusted_mulligan_measurement_v1",
        "into_authorization_gated_action_plan_v1",
        "execute_authorized_private_match_pregame_action_v3",
        "SendInput",
        "ReadProcessMemory",
        "WriteProcessMemory",
    ] {
        assert!(
            !source.contains(forbidden),
            "pinned visible-frame boundary exposes forbidden authority: {forbidden}"
        );
    }
}

#[test]
fn pregame_scoring_consumes_opaque_measurement_and_cannot_mint_input() {
    let source = include_str!("../src/probe.rs");
    let bottoming = include_str!("../src/probe/bottoming_model.rs");
    let bottoming_plan = include_str!("../src/probe/bottoming_model/action_plan.rs");
    let heuristic = include_str!("../src/probe/pregame_heuristic.rs");
    assert!(source.contains(
        "pub fn score_and_select_pregame_model_v3<S: MtgoExternalPregameScorerV3>(\n    measurement: OpaqueMtgoDxgiMulliganMeasurementV3,"
    ));
    assert!(source.contains(
        "pub fn validate_pregame_score_response_v3(\n    measurement: OpaqueMtgoDxgiMulliganMeasurementV3,"
    ));
    assert!(source.contains("fn validate_pregame_score_response_parts_v3("));
    assert!(!source.contains("pub fn validate_pregame_score_response_parts_v3("));
    assert!(!source.contains("pub fn make_live_input"));

    assert!(source.contains(
        "pub fn measure_mtgo_dxgi_mulligan_visible_hand_candidate_v3(\n    source: OpaqueMtgoDxgiMulliganMeasurementV3,\n    profile: CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1,"
    ));
    assert!(source.contains(
        "measurement: CheckedUntrustedMtgoOfflineMulliganVisibleCardIdentityCandidateV1,"
    ));
    assert!(source.contains(
        "pub fn score_and_select_card_aware_pregame_model_v4<S: MtgoExternalCardAwarePregameScorerV4>(\n    measurement: OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3,"
    ));
    assert!(source.contains(
        "pub fn validate_card_aware_pregame_score_response_v4(\n    measurement: OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3,"
    ));
    assert!(source.contains("ordered_visible_card_names: Vec<String>"));
    assert!(source.contains(
        "pub fn build_card_aware_pregame_action_plan_v4(\n    selection: OpaqueMtgoCardAwarePregameModelSelectionV4,"
    ));
    assert!(source.contains("CardAware(Box<OpaqueMtgoCardAwarePregameModelSelectionV4>)"));
    assert!(
        source.contains("the immediate visible card identities changed after card-aware scoring")
    );
    for required in [
        "pub fn start_card_aware_bottoming_session_v5(",
        "pub fn score_and_select_card_aware_bottoming_model_v5<",
        "pub fn confirm_card_aware_bottom_selection_v5(",
        "pub struct OpaqueMtgoCardAwareBottomingSessionV5 {",
        "pub struct OpaqueMtgoCardAwareBottomingModelSelectionV5 {",
        "MtgoBottomingCardIdentitySourceV5::ConfirmedActionHistory",
    ] {
        assert!(
            bottoming.contains(required),
            "bottoming model boundary is missing: {required}"
        );
    }
    for forbidden in [
        "SendInput",
        "mouse_event",
        "keybd_event",
        "PostMessage",
        "SendMessage",
        "target_point_client_px",
        "pub fn build_bottoming_action_plan",
        "safe_for_live_input_v5(&self) -> bool {\n        true",
    ] {
        assert!(
            !bottoming.contains(forbidden),
            "bottoming model boundary exposes forbidden authority: {forbidden}"
        );
    }
    for required in [
        "pub fn build_card_aware_bottoming_action_plan_v5(",
        "pub fn confirm_card_aware_bottoming_cancel_plan_v5(",
        "pub fn confirm_card_aware_bottoming_selection_plan_v5(",
        "pub fn confirm_card_aware_bottoming_submit_plan_v5(",
        "pub struct OpaqueMtgoBottomingActionPlanV5 {",
        "pub struct OpaqueMtgoConfirmedBottomingSubmitV5 {",
        "AllSelectionsReset",
    ] {
        assert!(
            bottoming_plan.contains(required),
            "bottoming action-plan boundary is missing: {required}"
        );
    }
    for forbidden in [
        "SendInput",
        "mouse_event",
        "keybd_event",
        "PostMessage",
        "SendMessage",
        "pub(crate) fn prepare_bottoming_actuation",
        "safe_for_live_input_v5(&self) -> bool {\n        true",
    ] {
        assert!(
            !bottoming_plan.contains(forbidden),
            "bottoming action plan exposes forbidden authority: {forbidden}"
        );
    }
    for required in [
        "pub struct MtgoNonModelPregameHeuristicV1 {",
        "impl MtgoExternalCardAwarePregameScorerV4 for MtgoNonModelPregameHeuristicV1",
        "impl MtgoExternalCardAwareBottomingScorerV5 for MtgoNonModelPregameHeuristicV1",
        "not-a-checkpoint-manifest",
        "not-model-parameters",
        "pub fn is_model_backed_v1(&self) -> bool",
        "pub fn safe_for_live_input_v1(&self) -> bool",
    ] {
        assert!(
            heuristic.contains(required),
            "non-model pregame heuristic is missing: {required}"
        );
    }
    for forbidden in [
        "safe_for_live_input_v1(&self) -> bool {\n        true",
        "is_model_backed_v1(&self) -> bool {\n        true",
        "SendInput",
        "mouse_event",
        "keybd_event",
        "PostMessage",
        "SendMessage",
        "ReadProcessMemory",
        "WriteProcessMemory",
        "target_point_client_px",
        "OpaqueMtgoPregameActionPlanV3",
        "OpaqueMtgoBottomingActionPlanV5",
    ] {
        assert!(
            !heuristic.contains(forbidden),
            "non-model pregame heuristic exposes forbidden authority: {forbidden}"
        );
    }
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
    assert!(source.contains("measurement: CheckedUntrustedMtgoOfflineFirstMainCandidateV2,"));
    assert!(source.contains(
        "pub fn measure_mtgo_dxgi_bottom_six_initial_candidate_v3(\n    source_frame: OpaqueMtgoDxgiFrameCandidateV3,"
    ));
    assert!(source.contains(
        "pub fn measure_mtgo_dxgi_bottom_six_state_candidate_v3(\n    source_frame: OpaqueMtgoDxgiFrameCandidateV3,"
    ));
    assert!(source.contains("pub struct OpaqueMtgoDxgiBottomSixStateMeasurementV3 {"));
    assert!(source.contains("measurement: CheckedUntrustedMtgoOfflineBottomSixStateCandidateV3,"));
    assert!(source.contains(
        "pub fn measure_mtgo_dxgi_bottom_six_reflow_candidate_v3(\n    before: OpaqueMtgoDxgiBottomSixStateMeasurementV3,\n    after: OpaqueMtgoDxgiBottomSixStateMeasurementV3,"
    ));
    assert!(source.contains("pub struct OpaqueMtgoDxgiBottomSixReflowMeasurementV3 {"));
    assert!(source.contains("measurement: CheckedUntrustedMtgoOfflineBottomSixReflowCandidateV1,"));
    assert!(source.contains(
        "pub fn measure_mtgo_dxgi_bottom_six_visible_card_identities_candidate_v3(\n    source: OpaqueMtgoDxgiBottomSixStateMeasurementV3,\n    profile: CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1,"
    ));
    assert!(source.contains("pub struct OpaqueMtgoDxgiBottomSixVisibleCardIdentityMeasurementV3 {"));
    assert!(
        source.contains("measurement: CheckedUntrustedMtgoOfflineVisibleCardIdentityCandidateV3,")
    );
    for current_classifier in [
        "classify_untrusted_offline_mulligan_ladder_candidate_v2",
        "classify_untrusted_offline_mulligan_visible_card_identities_v1",
        "classify_untrusted_offline_bottom_six_state_candidate_v3",
        "classify_untrusted_offline_bottom_six_reflow_candidate_v2",
        "classify_untrusted_offline_bottom_six_visible_card_identities_v3",
        "classify_untrusted_offline_first_main_candidate_v2",
    ] {
        assert!(
            source.contains(current_classifier),
            "live measurement seam is missing current classifier: {current_classifier}"
        );
    }
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
    let duel_runtime = include_str!("../src/probe/duel_perception_runtime.rs");
    let entry_runtime = include_str!("../src/probe/competitive_entry_runtime.rs");
    let navigation_runtime = include_str!("../src/probe/competitive_navigation_runtime.rs");
    for required in [
        "RATIFIED_PRIVATE_MATCH_AUTHORIZATION_COMMITMENT_V3: Option<&str> = None",
        "RATIFIED_COMPETITIVE_DUEL_PASS_AUTHORIZATION_COMMITMENT_V1: Option<&str> = None",
        "RATIFIED_COMPETITIVE_DUEL_PASS_AUTHORIZATION_FROM_REVIEW_COMMITMENT_V2: Option<&str> = None",
        "RATIFIED_COMPETITIVE_MATCH_LAUNCH_AUTHORIZATION_COMMITMENT_V1: Option<&str> = None",
        "ratify_private_match_authorization_v3",
        "ratify_competitive_duel_pass_authorization_v1",
        "ratify_competitive_duel_pass_authorization_from_correspondence_v2",
        "review_competitive_duel_pass_ratification_candidate_from_correspondence_v2",
        "review_competitive_entry_attended_v1",
        "review_competitive_entry_attended_v2",
        "review_competitive_entry_attended_v3",
        "review_competitive_entry_attended_v4",
        "OpaqueMtgoCompetitiveEntryReviewIdentityV1",
        "CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2",
        "CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3",
        "CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4",
        "MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3",
        "MtgoControlBoundCompetitiveEntryReviewCommitmentsV4",
        "CLASSIFIER_BOUND_COMPETITIVE_ENTRY_REVIEW_DOMAIN_V3",
        "CONTROL_BOUND_COMPETITIVE_ENTRY_REVIEW_DOMAIN_V4",
        "classifier_bound_owner_review_no_entry_no_spending_no_input",
        "source_identity_commitment_sha256",
        "source_navigation_classification_result_commitment_sha256",
        "CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1",
        "source_lifecycle_snapshot_commitment_sha256",
        "permission_review_commitment_sha256",
        "entry_authorization_sha256",
        "interactive_terminal_owner_review_no_entry_no_spending_no_input",
        "pub fn permits_event_entry_v1(&self) -> bool {\n        false",
        "pub fn permits_spending_v1(&self) -> bool {\n        false",
        "pub fn permits_event_entry_v2(&self) -> bool {\n        false",
        "pub fn permits_spending_v2(&self) -> bool {\n        false",
        "safe_for_live_input_v2(&self) -> bool {\n        false",
        "pub fn permits_event_entry_v3(&self) -> bool {\n        false",
        "pub fn permits_spending_v3(&self) -> bool {\n        false",
        "safe_for_live_input_v3(&self) -> bool {\n        false",
        "pub fn permits_event_entry_v4(&self) -> bool {\n        false",
        "pub fn permits_spending_v4(&self) -> bool {\n        false",
        "safe_for_live_input_v4(&self) -> bool {\n        false",
        "CheckedUntrustedMtgoAuthorizationCorrespondenceV1",
        "ratify_competitive_match_launch_v1",
        "ratify_competitive_match_launch_attended_v4",
        "OpaqueMtgoCompetitiveLaunchIdentityV1",
        "event_display_label",
        "opponent_display_name",
        "stdin.is_terminal()",
        "stdout.is_terminal()",
        "BCryptGenRandom",
        "begin_competitive_game_session_v1",
        "OpaqueMtgoCompetitiveGameSessionV1",
        "bind_prepared_competitive_duel_pass_session_v2",
        "execute_authorized_competitive_duel_pass_v1",
        "confirm_pending_competitive_duel_pass_v2",
        "advance_competitive_game_session_v1",
        "returned_only_after_newer_visible_postcondition",
        "postcondition_candidate_count",
        "mtgo_input_gate_status_v3",
        "OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1",
        "OpaqueMtgoPendingCompetitiveDuelPassV1",
        "competitive_mode_authorization_commitment_v1",
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
    for required in [
        "bind_opaque_navigation_frame_to_competitive_entry_review_identity_v1",
        "bind_classifier_backed_competitive_entry_control_dry_run_v1",
        "OpaqueMtgoCompetitiveEntryReviewIdentityV1",
        "OpaqueMtgoCompetitiveEntryControlDryRunV1",
        "MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1",
        "OPAQUE_COMPETITIVE_ENTRY_CONTROL_DRY_RUN_DOMAIN_V1",
        "visibly_enabled_confirmed",
        "OPAQUE_COMPETITIVE_CLASSIFIER_BOUND_ENTRY_REVIEW_IDENTITY_DOMAIN_V2",
        "visible_frame_region_content_sha256_v1",
        "source_window_mode != \"main_client\"",
        "source_capture_role != \"navigation\"",
        "opaque_composed_navigation_pixels_owner_review_only_no_entry_no_spending_no_input",
        "human_reviewed_confirm_entry_control_dry_run_no_join_no_spending_no_input",
    ] {
        assert!(
            entry_runtime.contains(required),
            "competitive entry identity is missing source binding: {required}"
        );
    }
    for forbidden in [
        "pub fn control_rect_client_px",
        "pub fn visible_control_label_v1",
        "pub fn execute_competitive_entry",
        "pub fn purchase_competitive_entry",
        "SendInput",
        "SetCursorPos",
    ] {
        assert!(
            !entry_runtime.contains(forbidden),
            "competitive entry dry run contains a forbidden exposure or actuator: {forbidden}"
        );
    }
    for required in [
        "bind_classified_navigation_frame_to_competitive_entry_review_identity_v1",
        "OpaqueMtgoRetainedCompetitiveNavigationClassificationV1",
        "classified navigation source-frame lineage changed",
        "classified navigation approved-account profile lineage changed",
    ] {
        assert!(
            navigation_runtime.contains(required),
            "competitive entry identity is missing classifier lineage: {required}"
        );
    }
    for required in [
        "bind_opaque_duel_perception_to_competitive_launch_identity_v1",
        "OpaqueMtgoCompetitiveLaunchIdentityV1",
        "visible_facts_v1()",
        "parse_competitive_duel_window_title_v1",
        "event_label_region_sha256",
    ] {
        assert!(
            duel_runtime.contains(required),
            "duel launch identity is missing required source binding: {required}"
        );
    }
    for forbidden in [
        "ratify_competitive_match_launch_attended_v2",
        "pub fn ratify_competitive_match_launch_attended_v3",
        "pub fn bind_prepared_competitive_duel_pass_authorization_v1",
        "pub fn confirm_pending_competitive_duel_pass_v1",
        "pub fn execute_competitive_entry",
        "pub fn purchase_competitive_entry",
        "RATIFIED_COMPETITIVE_ENTRY",
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

#[test]
fn correspondence_review_command_emits_commitments_without_private_text_or_authority() {
    let source = include_str!("../src/bin/review_mtgo_authorization_correspondence_v2.rs");
    for required in [
        "MAX_CORRESPONDENCE_BYTES_V2",
        "check_untrusted_authorization_correspondence_v1",
        "review_competitive_duel_pass_ratification_candidate_from_correspondence_v2",
        "private_correspondence_bytes_retained\": false",
        "account_alias_text_emitted\": false",
        "safe_for_live_input\": false",
        "permits_event_entry\": false",
        "permits_spending\": false",
    ] {
        assert!(
            source.contains(required),
            "correspondence review command is missing guard: {required}"
        );
    }
    for forbidden in [
        "println!(\"{visible_account_alias}",
        "println!(\"{correspondence_bytes",
        "fs::write(&correspondence_path, correspondence)",
    ] {
        assert!(
            !source
                .split("#[cfg(all(test, target_os = \"windows\"))]")
                .next()
                .unwrap()
                .contains(forbidden),
            "production correspondence review command exposes private input: {forbidden}"
        );
    }
}
