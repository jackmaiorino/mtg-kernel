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
        "LeagueAndChallengeLifecycleClassification",
        "classifier_assets_manifest_bytes",
        "MTGO_VISIBLE_COMPETITIVE_NAVIGATION_V1",
        "env_clear()",
        "Stdio::piped()",
        "child.kill()",
        "MAX_CLASSIFIER_RESPONSE_BYTES_V1",
        "check_untrusted_competitive_navigation_prediction_v1",
        "rehash_lifecycle_visible_facts_v1",
        "opaque_eighteen_slice_lifecycle_classification_no_entry_no_spending_no_input",
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
fn competitive_event_listing_is_current_pixel_bound_and_non_actionable() {
    let source = include_str!("../src/probe/competitive_event_listing_runtime.rs");
    let process_source = include_str!("../src/probe/competitive_navigation_runtime.rs");
    for required in [
        "bind_classified_navigation_frame_to_competitive_event_listing_v1",
        "classify_checked_untrusted_competitive_event_listing_v1",
        "bind_classified_competitive_event_listing_to_evaluation_v1",
        "AdmittedMtgoCompetitiveEventListingEvaluationV1",
        "check_untrusted_competitive_event_listing_pixels_v1",
        "check_untrusted_competitive_event_listing_classifier_request_v1",
        "OpaqueMtgoClassifiedCompetitiveNavigationFrameV1",
        "OpaqueMtgoClassifiedCompetitiveEventListingV1",
        "OpaqueMtgoEvaluatedCompetitiveEventListingV1",
        "OpaqueMtgoSourceBoundCompetitiveEventListingV1",
        "visible_frame_region_content_sha256_v1",
        "event_label_region_sha256",
        "open_entry_review_control_region_sha256",
        "approved_account_alias_sha256",
        "evaluation_ratification_commitment_sha256",
        "evaluation_admission_commitment_sha256",
        "opaque_current_pixels_no_open_review_no_entry_no_spending_no_input",
        "evaluated_listing_still_no_open_review_no_entry_no_spending_no_input",
        "prepare_opaque_competitive_event_listing_open_source_v1",
        "confirm_opaque_competitive_event_listing_opened_v1",
        "fresh_rehashed_event_browser_control_no_entry_confirmation_no_spending_no_public_coordinates",
        "strictly_newer_exact_entry_review_visible_no_entry_confirmation_no_spending_no_input",
        "permits_open_entry_review_v1(&self) -> bool {\n        false",
        "permits_event_entry_v1(&self) -> bool {\n        false",
        "permits_spending_v1(&self) -> bool {\n        false",
        "safe_for_input_v1(&self) -> bool {\n        false",
    ] {
        assert!(
            source.contains(required),
            "source-bound competitive event listing is missing: {required}"
        );
    }
    for required in [
        "--mtgo-visible-competitive-event-listing-v1",
        "MTGO_VISIBLE_COMPETITIVE_EVENT_LISTING_V1",
    ] {
        assert!(
            process_source.contains(required),
            "bounded event-listing process protocol is missing: {required}"
        );
    }
    for forbidden in [
        "pub fn canonical_bgra8",
        "pub fn control_rect_client_px",
        "pub fn click",
        "pub fn confirm_entry",
        "pub fn parser_control_rect",
        "permits_open_entry_review_v1(&self) -> bool {\n        true",
        "permits_event_entry_v1(&self) -> bool {\n        true",
        "permits_spending_v1(&self) -> bool {\n        true",
        "safe_for_input_v1(&self) -> bool {\n        true",
        "SendInput",
        "SetCursorPos",
        "ReadProcessMemory",
        "WriteProcessMemory",
    ] {
        assert!(
            !source.contains(forbidden),
            "source-bound competitive event listing exposes forbidden authority: {forbidden}"
        );
    }
}

#[test]
fn competitive_event_record_is_same_frame_pixel_bound_and_non_actionable() {
    let source = include_str!("../src/probe/competitive_event_record_runtime.rs");
    for required in [
        "bind_classified_navigation_frame_to_visible_event_record_v1",
        "OpaqueMtgoClassifiedCompetitiveNavigationFrameV1",
        "OpaqueMtgoSourceBoundCompetitiveEventRecordV1",
        "validate_visible_competitive_event_record_v1",
        "visible_frame_region_content_sha256_v1",
        "source_capture_commitment_sha256",
        "source_frame_profile_binding_sha256",
        "source_classification_result_commitment_sha256",
        "source_lifecycle_snapshot_commitment_sha256",
        "approved_account_alias_sha256",
        "event_identity_sha256",
        "exact_same_frame_event_record_pixels_rehashed_no_input_no_entry_no_spending_no_gameplay",
    ] {
        assert!(
            source.contains(required),
            "source-bound competitive event record is missing: {required}"
        );
    }
    for forbidden in [
        "pub fn canonical_bgra8",
        "pub fn visible_fact_rectangles",
        "pub fn input_command",
        "safe_for_live_input_v1(&self) -> bool {\n        true",
        "permits_event_entry_v1(&self) -> bool {\n        true",
        "permits_spending_v1(&self) -> bool {\n        true",
        "permits_gameplay_v1(&self) -> bool {\n        true",
        "SendInput",
        "ReadProcessMemory",
        "WriteProcessMemory",
    ] {
        assert!(
            !source.contains(forbidden),
            "source-bound competitive event record exposes forbidden authority: {forbidden}"
        );
    }
}

#[test]
fn competitive_event_record_parser_is_exact_frame_bounded_and_unratified() {
    let source = include_str!("../src/probe/competitive_event_record_runtime.rs");
    let process_source = include_str!("../src/probe/competitive_navigation_runtime.rs");
    let combined = format!("{source}\n{process_source}");
    let runtime_start = source
        .find("pub fn classify_checked_untrusted_competitive_event_record_v1")
        .expect("event-record parser runtime must exist");
    let runtime_end = source[runtime_start..]
        .find("\nfn parse_event_record_classifier_response_v1")
        .map(|offset| runtime_start + offset)
        .expect("event-record parser runtime must have a bounded source section");
    let runtime = &source[runtime_start..runtime_end];

    for required in [
        "OpaqueMtgoAdmittedCompetitiveNavigationFrameV1",
        "MtgoCompetitiveEventRecordClassifierProcessResponseV1",
        "league_and_challenge_eight_slice_event_record_checked_untrusted_v1",
        "--mtgo-visible-competitive-event-record-v1",
        "MTGO_VISIBLE_COMPETITIVE_EVENT_RECORD_V1",
        "validate_visible_competitive_lifecycle_snapshot_v1",
        "rehash_lifecycle_visible_facts_for_event_record_v1",
        "validate_visible_competitive_event_record_v1",
        "validate_event_record_visible_fact_pixels_v1",
        "checked_untrusted_event_record_parser_no_ratification_no_entry_no_spending_no_gameplay_no_input",
    ] {
        assert!(
            combined.contains(required),
            "bounded competitive event-record parser is missing: {required}"
        );
    }
    assert!(
        !runtime.contains("OpaqueMtgoClassifiedCompetitiveNavigationFrameV1"),
        "the exact-field parser must start from the admitted main-client frame, not a lifecycle label classification"
    );
    for forbidden in [
        "pub fn canonical_bgra8",
        "pub fn visible_fact_rectangles",
        "pub fn input_command",
        "safe_for_live_classification_v1(&self) -> bool {\n        true",
        "permits_event_entry_v1(&self) -> bool {\n        true",
        "permits_spending_v1(&self) -> bool {\n        true",
        "safe_for_input_v1(&self) -> bool {\n        true",
        "event_record_evaluation_ratification",
        "SendInput",
        "ReadProcessMemory",
        "WriteProcessMemory",
    ] {
        assert!(
            !combined.contains(forbidden),
            "bounded competitive event-record parser exposes forbidden authority: {forbidden}"
        );
    }
}

#[test]
fn competitive_lifecycle_controls_are_exact_frame_bound_and_non_actionable() {
    let source = include_str!("../src/probe/competitive_lifecycle_control_runtime.rs");
    for required in [
        "bind_classified_navigation_frame_to_lifecycle_control_v1",
        "OpaqueMtgoCompetitiveLifecycleControlV1",
        "PairingAcceptControlEnabled",
        "SideboardSubmitControlEnabled",
        "MatchContinueControlEnabled",
        "ReconnectResumeControlEnabled",
        "EventCloseControlEnabled",
        "visible_frame_region_content_sha256_v1",
        "confirm_opaque_competitive_lifecycle_control_postcondition_v1",
        "validate_checked_competitive_lifecycle_action_transition_v1",
        "strictly_newer_exact_action_postcondition_no_input_authority",
        "SideboardNoChangesConfirmed",
        "SideboardConfigurationVisible",
        "generic sideboard control binding requires an explicitly visible no-change state",
        "exact_enabled_control_detection_only_no_coordinates_no_input",
        "safe_for_live_input_v1(&self) -> bool {\n        false",
    ] {
        assert!(
            source.contains(required),
            "competitive lifecycle control seam is missing: {required}"
        );
    }
    for forbidden in [
        "pub fn rect_client_px",
        "pub fn canonical_bgra8",
        "pub fn input_command",
        "safe_for_live_input_v1(&self) -> bool {\n        true",
        "SendInput",
        "ReadProcessMemory",
        "WriteProcessMemory",
    ] {
        assert!(
            !source.contains(forbidden),
            "competitive lifecycle control seam exposes forbidden authority: {forbidden}"
        );
    }
}

#[test]
fn competitive_sideboard_measurement_is_pixel_bound_event_bound_and_non_actionable() {
    let parser = include_str!("../src/probe/competitive_sideboard_runtime.rs");
    for required in [
        "check_untrusted_competitive_sideboard_classifier_request_v1",
        "classify_checked_untrusted_competitive_sideboard_v1",
        "plan_classified_competitive_sideboard_v1",
        "OpaqueMtgoClassifiedCompetitiveSideboardV1",
        "OpaqueMtgoPlannedCompetitiveSideboardV1",
        "source_navigation_classification_result_commitment_sha256",
        "source_lifecycle_snapshot_commitment_sha256",
        "deck_list_sha256",
        "deck_manifest_commitment_sha256",
        "deck_format_sha256",
        "policy_deployment_commitment_sha256",
        "visible_frame_region_content_sha256_v1",
        "same_frame_rehashed_sideboard_checked_untrusted_no_input_no_submit",
        "coordinate_free_model_sideboard_plan_no_input_no_submit",
        "safe_for_input_v1(&self) -> bool {\n        false",
        "permits_sideboard_submission_v1(&self) -> bool {\n        false",
    ] {
        assert!(
            parser.contains(required),
            "sideboard measurement seam is missing: {required}"
        );
    }
    for forbidden in [
        "pub fn canonical_bgra8",
        "pub fn card_rectangles",
        "pub fn input_command",
        "safe_for_input_v1(&self) -> bool {\n        true",
        "SendInput",
        "ReadProcessMemory",
        "WriteProcessMemory",
    ] {
        assert!(
            !parser.contains(forbidden),
            "sideboard measurement exposes forbidden authority: {forbidden}"
        );
    }

    let coordinator = include_str!("../src/actuator.rs");
    for required in [
        "measure_competitive_event_runtime_sideboard_v1",
        "plan_measured_competitive_event_sideboard_v1",
        "OpaqueMtgoMeasuredCompetitiveEventSideboardV1",
        "OpaqueMtgoPlannedCompetitiveEventSideboardV1",
        "COMPETITIVE_EVENT_SIDEBOARD_MEASUREMENT_DOMAIN_V1",
        "COMPETITIVE_EVENT_SIDEBOARD_PLAN_DOMAIN_V1",
        "event_runtime_withheld_during_checked_untrusted_sideboard_measurement_no_input_no_submit",
        "coordinate_free_event_bound_sideboard_plan_no_input_no_submit",
        "manifest.deck_list_sha256() != runtime.commitments.deck_manifest_sha256",
        "manifest.format_sha256() != runtime.commitments.deck_format_sha256",
        "sideboard.policy_deployment_commitment_sha256",
        "begin_competitive_event_sideboard_transfer_sequence_v1",
        "prepare_competitive_event_sideboard_transfer_drag_v1",
        "confirm_competitive_event_sideboard_transfer_visible_v1",
        "review_competitive_sideboard_automation_ratification_candidate_v1",
        "ratify_competitive_sideboard_automation_v1",
        "AdmittedMtgoCompetitiveSideboardEvaluationV1",
        "sideboard_evaluation_ratification_commitment_sha256",
        "sideboard_evaluation_admission_commitment_sha256",
        "sideboard evaluation differs from the exact lifecycle profile, account, deck, format, or policy",
        "RATIFIED_COMPETITIVE_SIDEBOARD_AUTOMATION_COMMITMENT_V1: Option<&str> = None",
        "COMPETITIVE_SIDEBOARD_AUTOMATION_SCOPE_DOMAIN_V1",
        "OpaqueMtgoCompetitiveEventSideboardSequenceV1",
        "OpaqueMtgoPreparedCompetitiveEventSideboardTransferV1",
        "OpaqueMtgoReadyCompetitiveEventSideboardV1",
        "OpaqueMtgoFreshPreparedCompetitiveEventSideboardDragV1",
        "OpaqueMtgoPendingCompetitiveEventSideboardDragV1",
        "OpaqueMtgoConfirmedCompetitiveEventSideboardDragV1",
        "prepare_fresh_competitive_event_sideboard_transfer_drag_v1",
        "execute_fresh_competitive_event_sideboard_drag_v1",
        "confirm_pending_competitive_event_sideboard_drag_v1",
        "send_exactly_one_sideboard_drag_v1",
        "prepare_ready_competitive_event_sideboard_submit_v1",
        "sideboard_to_mainboard_first_one_card_per_step_visible_confirmation_required_no_input",
        "official_mtgo_drag_between_visible_zones_preparation_only_no_input",
        "exactly_one_newer_visible_sideboard_transfer_no_causality_no_input",
        "all_model_selected_sideboard_transfers_visibly_confirmed_no_submit_no_input",
        "official_mtgo_drag_between_visible_zones_one_card_per_input",
        "strictly_newer_exact_inventory_confirmation_after_each_drag",
        "changed_sideboard_submit_only_after_exact_target_ready",
        "no_double_click_no_keyboard_no_hidden_channels_no_event_entry_no_spending",
        "halt_before_input_attempt_v3",
        "set_pending_v3",
        "release_confirmed_pending_v3",
    ] {
        assert!(
            coordinator.contains(required),
            "event sideboard coordinator is missing: {required}"
        );
    }

    for forbidden in [
        "RATIFIED_COMPETITIVE_SIDEBOARD_AUTOMATION_COMMITMENT_V1: Option<&str> = Some",
        "mouse_event",
        "keybd_event",
        "PostMessage",
        "SendMessage",
        "ReadProcessMemory",
        "WriteProcessMemory",
    ] {
        assert!(
            !coordinator.contains(forbidden),
            "event sideboard coordinator exposes a forbidden authority or channel: {forbidden}"
        );
    }
}

#[test]
fn competitive_event_runtime_is_move_only_identity_bound_and_terminal_record_gated() {
    let source = include_str!("../src/actuator.rs");
    for required in [
        "OpaqueMtgoCompetitiveEventRuntimeV1",
        "begin_competitive_event_runtime_after_entry_v1",
        "advance_competitive_event_runtime_observed_v1",
        "prepare_competitive_event_runtime_lifecycle_control_v1",
        "execute_prepared_competitive_event_lifecycle_control_v1",
        "confirm_pending_competitive_event_lifecycle_control_v1",
        "attach_competitive_event_monitor_to_runtime_v1",
        "advance_competitive_event_monitor_in_runtime_v1",
        "bind_competitive_event_runtime_to_match_launch_identity_v1",
        "OpaqueMtgoCompetitiveEventMatchLaunchBindingV1",
        "COMPETITIVE_EVENT_MATCH_LAUNCH_BINDING_DOMAIN_V1",
        "paid_main_client_event_runtime_bound_to_newer_same_process_duel_launch_no_input_no_spending",
        "process_continuity_commitment_sha256_v1",
        "checkout_competitive_event_gameplay_session_v1",
        "return_competitive_event_gameplay_session_v1",
        "COMPETITIVE_GESTURE_GAME_SESSION_EVENT_DECK_BIND_DOMAIN_V1",
        "competitive_gesture_game_session_event_deck_binding_commitment_v1",
        "exact_event_entry_selected_deck_bound_to_all_family_game_session",
        "gesture session is not bound to an exact event entry and selected deck",
        "gameplay.entry_authorization_sha256 != runtime.entry_authorization_sha256",
        "game.correspondence_sha256 != runtime.correspondence_sha256",
        "game.permission_review_commitment_sha256",
        "competitive entry and lifecycle authorities do not share one exact reviewed permission lineage",
        "deck_manifest_sha256",
        "deck_format_sha256",
        "selected_deck_label_sha256",
        "policy_deployment_commitment_sha256",
        "last_returned_gameplay_frame_sequence",
        "validate_competitive_event_next_frame_order_v1",
        "competitive event next frame is not newer than its lifecycle and returned-gameplay floors",
        "current exact entry, selected deck, event, match, game, account, or frame lifetime",
        "closing a competitive event requires its terminal visible event record",
        "move_only_gameplay_lease_event_runtime_withheld",
        "one_exact_event_move_only_no_reentry_no_additional_spending",
        "permits_additional_entry_v1(&self) -> bool {\n        false",
        "permits_additional_spending_v1(&self) -> bool {\n        false",
    ] {
        assert!(
            source.contains(required),
            "competitive event runtime is missing: {required}"
        );
    }
    for forbidden in [
        "pub fn current_frame_v1",
        "pub fn lifecycle_authorization_v1",
        "pub fn target_point_client_px",
        "pub fn input_command",
    ] {
        assert!(
            !source.contains(forbidden),
            "competitive event runtime exposes a forbidden capability: {forbidden}"
        );
    }
}

#[test]
fn competitive_event_monitor_is_move_only_monotonic_and_non_actionable() {
    let source = include_str!("../src/probe/competitive_event_record_runtime.rs");
    for required in [
        "OpaqueMtgoCompetitiveEventMonitorV1",
        "begin_checked_untrusted_competitive_event_monitor_v1",
        "advance_checked_untrusted_competitive_event_monitor_v1",
        "validate_event_monitor_advance_v1",
        "validate_event_monitor_progress_advance_v1",
        "competitive event monitor identity changed across records",
        "competitive event monitor requires a changed strictly newer record",
        "a completed competitive event monitor cannot accept another record",
        "checked_untrusted_event_monitor_advance_no_entry_no_spending_no_gameplay_no_input",
        "permits_gameplay_v1(&self) -> bool {\n        false",
    ] {
        assert!(
            source.contains(required),
            "competitive event monitor is missing: {required}"
        );
    }
    for forbidden in [
        "impl Clone for OpaqueMtgoCompetitiveEventMonitorV1",
        "pub fn canonical_bgra8",
        "pub fn visible_fact_rectangles",
        "pub fn input_command",
        "safe_for_live_classification_v1(&self) -> bool {\n        true",
        "permits_event_entry_v1(&self) -> bool {\n        true",
        "permits_spending_v1(&self) -> bool {\n        true",
        "permits_gameplay_v1(&self) -> bool {\n        true",
        "safe_for_input_v1(&self) -> bool {\n        true",
        "SendInput",
        "ReadProcessMemory",
        "WriteProcessMemory",
    ] {
        assert!(
            !source.contains(forbidden),
            "competitive event monitor exposes forbidden authority: {forbidden}"
        );
    }
}

#[test]
fn opaque_duel_perception_runtime_retains_pixels_through_scoring_without_input_authority() {
    let source = include_str!("../src/probe/duel_perception_runtime.rs");
    let lifecycle_evaluation =
        include_str!("../../mtgo_blackbox_v1/src/competitive_duel_lifecycle_evaluation.rs");
    for required in [
        "verify_duel_perception_runtime_v1",
        "verify_duel_gesture_target_runtime_v1",
        "perceive_admitted_duel_frame_v1",
        "score_and_select_opaque_admitted_duel_perception_v1",
        "scorer: &mut MtgoNativeCheckpointObservationScorerV1<'_>",
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
        "MtgoDuelGestureTargetProcessResponseV1",
        "MtgoDuelGestureTargetRequestHeaderV1",
        "check_untrusted_duel_perception_request_v1",
        "check_untrusted_duel_gesture_target_request_v1",
        "CheckedUntrustedMtgoDuelPerceptionRequestV1",
        "CheckedUntrustedMtgoDuelGestureTargetRequestV1",
        "request_commitment_sha256",
        "reconstruction_audit: MtgoObservationReconstructionAuditV1",
        "visible_controls: MtgoVisibleActionControlSetV1",
        "competitive_lifecycle: Option<MtgoVisibleCompetitiveLifecycleSnapshotV1>",
        "competitive_lifecycle_snapshot_commitment_sha256: Option<String>",
        "AdmittedMtgoCompetitiveDuelLifecycleProfileV1",
        "lifecycle_evaluation_commitment_sha256",
        "lifecycle_profile_admission_commitment_sha256",
        "competitive duel action requires classifier-bound lifecycle pixels",
        "visible_frame_region_content_sha256_v1",
        "validate_observed_decision_v1",
        "check_untrusted_dxgi_capture_artifact_v1",
        "validate_dxgi_bound_observation_reconstruction_audit_v1",
        "check_untrusted_dxgi_observed_decision_candidate_v1",
        "score_and_select_profile_bound_duel_candidate_v1",
        "resolve_profile_bound_selected_visible_control_v1",
        "prepare_profile_bound_action_postcondition_plan_v1",
        "validate_profile_bound_duel_gesture_plan_v1",
        "bind_profile_bound_action_plan_to_competitive_match_v1",
        "gesture_plan_commitment_sha256",
        "gesture_stage_count",
        "bind_opaque_competitive_duel_source_gesture_stage_v1",
        "bind_opaque_competitive_duel_continuation_gesture_stage_v1",
        "begin_opaque_competitive_duel_gesture_sequence_v1",
        "advance_opaque_competitive_duel_gesture_sequence_v1",
        "prepare_opaque_competitive_duel_gesture_source_stage_from_fresh_frame_v1",
        "prepare_opaque_competitive_duel_gesture_source_stage_from_pinned_runtime_v1",
        "OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1",
        "MTGO_VISIBLE_DUEL_GESTURE_TARGET_V1",
        "gesture_target_runtime_binary_sha256",
        "gesture_target_assets_manifest_sha256",
        "gesture_target_request_commitment_sha256",
        "recheck_visible_duel_gesture_source_stage_v1",
        "fresh_source_stage_rechecked_private_points_no_input_or_action_causality",
        "MtgoCompetitiveDuelGestureVisibleTransitionProbeV1",
        "OpaqueMtgoCompetitiveDuelGestureSequenceV1",
        "gesture visible-transition probe region did not change",
        "one_adjacent_stage_visible_region_changed_no_input_or_action_causality",
        "bind_visible_duel_gesture_stage_v1",
        "OpaqueMtgoCompetitiveDuelGestureStageV1",
        "target_points_desktop_px",
        "duel_action_family_v1",
        "profile.supported_action_families()",
        "source_frame: OpaqueMtgoAdmittedDuelVisibleFrameV1",
        "perception: OpaqueMtgoAdmittedDuelPerceptionV1",
        "env_clear()",
        "MAX_PERCEPTION_RESPONSE_BYTES_V1",
        "runtime timed out",
        "safe_for_input_v1(&self) -> bool",
        "permits_event_entry_v1(&self) -> bool",
        "proves_action_causality_v1(&self) -> bool",
    ] {
        assert!(
            source.contains(required),
            "opaque duel-perception runtime is missing: {required}"
        );
    }
    assert!(!source.contains("pub fn bind_opaque_duel_control_to_competitive_action_plan_v1("));
    assert!(!source.contains(
        "score_and_select_opaque_admitted_duel_perception_v1<S: MtgoExternalObservationScorerV1>"
    ));
    for required in [
        "RATIFIED_COMPETITIVE_DUEL_LIFECYCLE_EVALUATION_COMMITMENT_V1: Option<&str> = None",
        "minimum_unique_cases_per_mode",
        "MtgoCompetitiveEventKindV1::League",
        "MtgoCompetitiveEventKindV1::Challenge",
        "exact_snapshot_count == prediction_count",
        "league_and_challenge_match_in_progress_lifecycle_accuracy_only_no_input_or_entry",
    ] {
        assert!(
            lifecycle_evaluation.contains(required),
            "competitive duel lifecycle gate is missing: {required}"
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
        "proves_action_causality_v1(&self) -> bool {\n        true",
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
        "RATIFIED_COMPETITIVE_DUEL_GESTURE_AUTHORIZATION_FROM_REVIEW_COMMITMENT_V1",
        "RATIFIED_COMPETITIVE_SELECTED_LISTING_ENTRY_AUTHORIZATION_COMMITMENT_V2: Option<&str> = None",
        "RATIFIED_COMPETITIVE_LIFECYCLE_AUTHORIZATION_COMMITMENT_V1: Option<&str> = None",
        "RATIFIED_COMPETITIVE_OPEN_ENTRY_REVIEW_AUTHORIZATION_COMMITMENT_V1: Option<&str> = None",
        "RATIFIED_COMPETITIVE_MATCH_LAUNCH_AUTHORIZATION_COMMITMENT_V1: Option<&str> = None",
        "ratify_private_match_authorization_v3",
        "ratify_competitive_duel_pass_authorization_v1",
        "ratify_competitive_duel_pass_authorization_from_correspondence_v2",
        "review_competitive_duel_pass_ratification_candidate_from_correspondence_v2",
        "ratify_competitive_duel_gesture_authorization_from_correspondence_v1",
        "review_competitive_duel_gesture_ratification_candidate_from_correspondence_v1",
        "RatifiedMtgoCompetitiveDuelGestureAuthorizationV1",
        "MtgoReviewedCompetitiveGestureRatificationCandidateV1",
        "canonical_duel_gesture_action_families_v1",
        "complete_reviewed_eleven_family_profile_permission_identity_only_no_input_entry_or_spending",
        "COMPETITIVE_DUEL_GESTURE_AUTHORIZATION_FROM_REVIEW_DOMAIN_V1",
        "review_competitive_entry_attended_v1",
        "review_competitive_entry_attended_v2",
        "review_competitive_entry_attended_v3",
        "review_competitive_entry_attended_v4",
        "review_competitive_entry_ratification_candidate_v1",
        "bind_confirmed_competitive_open_entry_review_to_entry_review_v1",
        "review_competitive_entry_ratification_candidate_from_selected_listing_v2",
        "ratify_competitive_entry_authorization_from_selected_listing_v2",
        "CheckedUntrustedMtgoSelectedListingBoundCompetitiveEntryReviewV1",
        "selected_listing_visible_arrival_to_fresh_exact_paid_entry_review_no_entry_no_spending_no_input",
        "prepare_ratified_competitive_entry_v1",
        "execute_prepared_competitive_entry_v1",
        "confirm_pending_competitive_entry_v1",
        "RatifiedMtgoCompetitiveEntryAuthorizationV1",
        "OpaqueMtgoPreparedCompetitiveEntryV1",
        "OpaqueMtgoPendingCompetitiveEntryV1",
        "OpaqueMtgoConfirmedCompetitiveEntryV1",
        "MtgoPreparedCompetitiveEntryCommitmentsV1",
        "MtgoCompetitiveEntryInputReceiptCommitmentsV1",
        "MtgoConfirmedCompetitiveEntryCommitmentsV1",
        "MtgoReviewedCompetitiveEntryRatificationCandidateV1",
        "review_competitive_lifecycle_ratification_candidate_from_correspondence_v1",
        "ratify_competitive_lifecycle_authorization_from_correspondence_v1",
        "prepare_ratified_competitive_lifecycle_control_v1",
        "execute_prepared_competitive_lifecycle_control_v1",
        "confirm_pending_competitive_lifecycle_control_v1",
        "RatifiedMtgoCompetitiveLifecycleAuthorizationV1",
        "OpaqueMtgoPreparedCompetitiveLifecycleControlV1",
        "OpaqueMtgoPendingCompetitiveLifecycleControlV1",
        "OpaqueMtgoConfirmedCompetitiveLifecycleControlV1",
        "COMPETITIVE_LIFECYCLE_AUTHORIZATION_DOMAIN_V1",
        "COMPETITIVE_LIFECYCLE_PREPARATION_DOMAIN_V1",
        "COMPETITIVE_LIFECYCLE_INPUT_RECEIPT_DOMAIN_V1",
        "COMPETITIVE_LIFECYCLE_CONFIRMATION_RECEIPT_DOMAIN_V1",
        "all_five_non_entry_lifecycle_controls_one_click_each_exact_postcondition",
        "exactly_one_left_click_shared_gate_pending_visible_postcondition",
        "exact_action_visibly_confirmed_shared_gate_released",
        "review_competitive_open_entry_review_ratification_candidate_v1",
        "ratify_competitive_open_entry_review_authorization_v1",
        "prepare_ratified_competitive_open_entry_review_v1",
        "execute_prepared_competitive_open_entry_review_v1",
        "confirm_pending_competitive_open_entry_review_v1",
        "RatifiedMtgoCompetitiveOpenEntryReviewAuthorizationV1",
        "OpaqueMtgoPreparedCompetitiveOpenEntryReviewV1",
        "OpaqueMtgoPendingCompetitiveOpenEntryReviewV1",
        "OpaqueMtgoConfirmedCompetitiveOpenEntryReviewV1",
        "COMPETITIVE_OPEN_ENTRY_REVIEW_AUTHORIZATION_DOMAIN_V1",
        "COMPETITIVE_OPEN_ENTRY_REVIEW_PREPARATION_DOMAIN_V1",
        "COMPETITIVE_OPEN_ENTRY_REVIEW_INPUT_RECEIPT_DOMAIN_V1",
        "COMPETITIVE_OPEN_ENTRY_REVIEW_CONFIRMATION_RECEIPT_DOMAIN_V1",
        "one_fresh_evaluated_selected_listing",
        "exactly_one_open_review_left_click_shared_gate_pending_visible_entry_review",
        "exact_entry_review_visibly_confirmed_shared_gate_released_no_entry_no_spending",
        "COMPETITIVE_ENTRY_AUTHORIZATION_RATIFICATION_DOMAIN_V1",
        "COMPETITIVE_ENTRY_PREPARATION_DOMAIN_V1",
        "COMPETITIVE_ENTRY_INPUT_RECEIPT_DOMAIN_V1",
        "COMPETITIVE_ENTRY_CONFIRMATION_RECEIPT_DOMAIN_V1",
        "exact_owner_reviewed_existing_account_resource_and_selected_deck_entry_requires_fresh_recapture_and_visible_postcondition",
        "ratified_exact_entry_fresh_visible_review_retained_no_input_no_join_no_spending",
        "exactly_one_exactly_ratified_competitive_entry_left_click_pending_visible_confirmation",
        "one_entry_input_visible_entered_waiting_confirmed_shared_gate_released",
        "OpaqueMtgoCompetitiveEntryReviewIdentityV1",
        "CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2",
        "CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3",
        "CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4",
        "MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3",
        "MtgoControlBoundCompetitiveEntryReviewCommitmentsV4",
        "CLASSIFIER_BOUND_COMPETITIVE_ENTRY_REVIEW_DOMAIN_V3",
        "CONTROL_BOUND_COMPETITIVE_ENTRY_REVIEW_DOMAIN_V4",
        "bind_competitive_entry_postcondition_dry_run_v1",
        "CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1",
        "MtgoCompetitiveEntryPostconditionDryRunCommitmentsV1",
        "COMPETITIVE_ENTRY_POSTCONDITION_DRY_RUN_DOMAIN_V1",
        "visible_postcondition_calibration_pair_no_causality_no_join_no_spending_no_input",
        "pub fn claims_action_causality_v1(&self) -> bool {\n        false",
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
        "ratify_competitive_event_match_launch_attended_v5",
        "ratify_competitive_gesture_match_launch_attended_v1",
        "RatifiedMtgoCompetitiveGestureMatchLaunchV1",
        "ATTENDED_COMPETITIVE_GESTURE_MATCH_LAUNCH_UPGRADE_DOMAIN_V1",
        "owner_extends_exact_pass_launch_to_complete_reviewed_eleven_family_profile_no_entry_or_spending",
        "OpaqueMtgoCompetitiveLaunchIdentityV1",
        "event_display_label",
        "opponent_display_name",
        "stdin.is_terminal()",
        "stdout.is_terminal()",
        "BCryptGenRandom",
        "begin_competitive_game_session_v1",
        "OpaqueMtgoCompetitiveGameSessionV1",
        "begin_competitive_gesture_game_session_v1",
        "OpaqueMtgoCompetitiveGestureGameSessionV1",
        "MtgoCompetitiveGestureGameSessionCommitmentsV1",
        "COMPETITIVE_GESTURE_GAME_SESSION_INITIAL_DOMAIN_V1",
        "move_only_all_family_lineage_no_preparation_execution_entry_or_spending",
        "bind_competitive_duel_gesture_sequence_session_v1",
        "OpaqueMtgoSessionBoundCompetitiveDuelGestureV1",
        "MtgoSessionBoundCompetitiveDuelGestureCommitmentsV1",
        "COMPETITIVE_GESTURE_SESSION_SEQUENCE_BINDING_DOMAIN_V1",
        "one_source_stage_bound_to_exact_all_family_session_no_preparation_execution_or_input",
        "prepare_session_bound_competitive_duel_gesture_source_stage_v1",
        "OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1",
        "MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1",
        "COMPETITIVE_GESTURE_SESSION_SOURCE_PREPARATION_DOMAIN_V1",
        "source_primitive_freshly_rechecked_pinned_runtime_targets_no_input",
        "target_runtime_attested_v1(&self) -> bool",
        "execute_prepared_competitive_duel_gesture_primitive_v1",
        "confirm_pending_competitive_duel_gesture_primitive_v1",
        "OpaqueMtgoPendingCompetitiveDuelGesturePrimitiveV1",
        "OpaqueMtgoConfirmedCompetitiveDuelGesturePrimitiveV1",
        "COMPETITIVE_DUEL_GESTURE_INPUT_RECEIPT_DOMAIN_V1",
        "COMPETITIVE_DUEL_GESTURE_TRANSITION_RECEIPT_DOMAIN_V1",
        "COMPETITIVE_DUEL_GESTURE_CONTINUATION_RECEIPT_DOMAIN_V1",
        "COMPETITIVE_GESTURE_SESSION_CONTINUATION_PREPARATION_DOMAIN_V1",
        "COMPETITIVE_GESTURE_GAME_SESSION_ADVANCE_DOMAIN_V1",
        "confirm_pending_competitive_duel_gesture_continuation_v1",
        "prepare_confirmed_competitive_duel_gesture_continuation_stage_v1",
        "pending_primitive_joined_to_exact_newer_runtime_pinned_visible_stage_no_next_input",
        "confirmed_transition_then_distinct_fresh_stage_recheck_no_input",
        "send_exactly_one_gesture_primitive_v1",
        "MOUSEEVENTF_RIGHTDOWN",
        "MOUSEEVENTF_RIGHTUP",
        "all_family_session_returned_only_after_newer_visible_postcondition",
        "before_input_postcondition_verification_commitment_sha256",
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
    assert!(!source.contains("pub fn ratify_competitive_entry_authorization_v1("));
    assert!(!source.contains("pub fn ratify_competitive_match_launch_v1("));
    assert!(!source.contains("pub fn ratify_competitive_match_launch_attended_v4("));
    assert!(!duel_runtime
        .contains("pub fn bind_opaque_duel_perception_to_competitive_launch_identity_v1("));
    for required in [
        "bind_opaque_navigation_frame_to_competitive_entry_review_identity_v1",
        "bind_classifier_backed_competitive_entry_control_and_deck_dry_run_v2",
        "MtgoCompetitiveEntryDeckSelectionReviewInputV1",
        "OpaqueMtgoCompetitiveEntryReviewIdentityV1",
        "OpaqueMtgoCompetitiveEntryControlDryRunV1",
        "MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1",
        "OPAQUE_COMPETITIVE_ENTRY_CONTROL_AND_DECK_DRY_RUN_DOMAIN_V2",
        "visibly_enabled_confirmed",
        "selected_deck_label_sha256",
        "selected_deck_region_sha256",
        "deck_manifest_sha256",
        "deck_format_sha256",
        "policy_deployment_commitment_sha256",
        "OPAQUE_COMPETITIVE_CLASSIFIER_BOUND_ENTRY_REVIEW_IDENTITY_DOMAIN_V2",
        "visible_frame_region_content_sha256_v1",
        "source_window_mode != \"main_client\"",
        "source_capture_role != \"navigation\"",
        "opaque_composed_navigation_pixels_owner_review_only_no_entry_no_spending_no_input",
        "human_reviewed_confirm_entry_control_and_selected_deck_dry_run_no_join_no_spending_no_input",
        "validate_classifier_backed_competitive_entry_frame_transition_v1",
        "MtgoCompetitiveEntryFrameTransitionCommitmentsV1",
        "COMPETITIVE_ENTRY_FRAME_TRANSITION_DOMAIN_V1",
        "EnteredWaitingForPairing",
        "entry_review_to_entered_waiting_visible_pair_no_causality_no_entry_no_spending_no_input",
        "validate_classifier_backed_competitive_entry_immediate_recapture_v1",
        "MtgoCompetitiveEntryImmediateRecaptureCommitmentsV1",
        "COMPETITIVE_ENTRY_IMMEDIATE_RECAPTURE_DOMAIN_V1",
        "fresh_entry_review_event_terms_enabled_control_and_selected_deck_exact_no_input_no_entry_no_spending",
        "confirm_opaque_competitive_entry_postcondition_v1",
        "COMPETITIVE_ENTRY_VISIBLE_CONFIRMATION_DOMAIN_V1",
        "entered_waiting_confirmed_after_exactly_one_entry_input",
    ] {
        assert!(
            entry_runtime.contains(required),
            "competitive entry identity is missing source binding: {required}"
        );
    }
    for forbidden in [
        "bind_classifier_backed_competitive_entry_control_dry_run_v1",
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
        "confirm_opaque_competitive_duel_gesture_postcondition_v1",
        "DUEL_OPAQUE_COMPETITIVE_GESTURE_CONFIRMATION_DOMAIN_V1",
        "competitive gesture postcondition capture predates its input receipt",
        "advance_opaque_competitive_duel_gesture_sequence_from_pinned_runtime_v1",
        "OpaqueMtgoPinnedCompetitiveDuelGestureContinuationV1",
        "MtgoOpaquePinnedCompetitiveDuelGestureContinuationCommitmentsV1",
        "DUEL_OPAQUE_PINNED_COMPETITIVE_GESTURE_CONTINUATION_DOMAIN_V1",
        "invoke_pinned_gesture_target_runtime_for_stage_v1",
        "derive_opaque_gesture_visible_transition_probe_v1",
        "no changed same-rectangle evidence ties the adjacent gesture stages",
        "one_runtime_pinned_adjacent_visible_stage_no_input_or_action_causality",
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
