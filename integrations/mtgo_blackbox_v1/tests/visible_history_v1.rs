use mtgo_blackbox_v1::{
    build_checked_untrusted_visible_history_v1, validate_gameplay_calibration_trace_v1,
    validate_pregame_calibration_trace_v1, validate_visible_object_action_calibration_trace_v1,
    CheckedUntrustedMtgoGameplayCalibrationV1, CheckedUntrustedMtgoPregameCalibrationV1,
    CheckedUntrustedMtgoVisibleObjectActionCalibrationV1,
    MtgoCheckedUntrustedCalibrationTransitionRefV1 as Source, MtgoGameplayCalibrationTraceV1,
    MtgoPregameCalibrationTraceV1, MtgoVisibleHistoryLinkV1,
    MtgoVisibleObjectActionCalibrationTraceV1,
};

fn keep() -> CheckedUntrustedMtgoPregameCalibrationV1 {
    let record: MtgoPregameCalibrationTraceV1 = serde_json::from_str(include_str!(
        "../fixtures/solitaire_keep_transition_v1.json"
    ))
    .unwrap();
    validate_pregame_calibration_trace_v1(record).unwrap()
}

fn pass() -> CheckedUntrustedMtgoGameplayCalibrationV1 {
    let record: MtgoGameplayCalibrationTraceV1 = serde_json::from_str(include_str!(
        "../fixtures/solitaire_pass_to_combat_transition_v1.json"
    ))
    .unwrap();
    validate_gameplay_calibration_trace_v1(record).unwrap()
}

fn play_land() -> CheckedUntrustedMtgoVisibleObjectActionCalibrationV1 {
    let record: MtgoVisibleObjectActionCalibrationTraceV1 = serde_json::from_str(include_str!(
        "../fixtures/solitaire_play_land_transition_v1.json"
    ))
    .unwrap();
    validate_visible_object_action_calibration_trace_v1(record).unwrap()
}

fn mana() -> CheckedUntrustedMtgoVisibleObjectActionCalibrationV1 {
    let record: MtgoVisibleObjectActionCalibrationTraceV1 = serde_json::from_str(include_str!(
        "../fixtures/solitaire_activate_island_mana_transition_v1.json"
    ))
    .unwrap();
    validate_visible_object_action_calibration_trace_v1(record).unwrap()
}

fn gap(reason: &str) -> MtgoVisibleHistoryLinkV1 {
    MtgoVisibleHistoryLinkV1::VisibleGap {
        reason_codes: vec![reason.to_owned()],
    }
}

#[test]
fn live_solitaire_history_records_two_gaps_and_one_exact_link() {
    let keep = keep();
    let pass = pass();
    let land = play_land();
    let mana = mana();
    let sources = [
        Source::Pregame(&keep),
        Source::Gameplay(&pass),
        Source::VisibleObject(&land),
        Source::VisibleObject(&mana),
    ];
    let links = [
        MtgoVisibleHistoryLinkV1::StartOfCapturedHistory,
        gap("uncaptured_priority_transitions_after_keep"),
        gap("uncaptured_turn_progression_before_play_land"),
        MtgoVisibleHistoryLinkV1::ExactFrameMatch,
    ];
    let checked = build_checked_untrusted_visible_history_v1(
        "solitaire_supervised_history",
        &sources,
        &links,
    )
    .unwrap();

    assert_eq!(checked.step_count(), 4);
    assert_eq!(checked.gap_count(), 2);
    assert_eq!(checked.trailing_exact_step_count(), 2);
    assert!(checked.begins_at_opening_hand_decision());
    assert!(!checked.exact_pixel_chain_from_opening_hand());
    assert!(!checked.safe_for_kernel_context());
    assert_eq!(checked.history_commitment_sha256().len(), 64);
}

#[test]
fn exact_and_gap_links_must_match_source_frame_hashes() {
    let keep = keep();
    let pass = pass();
    let mismatched_sources = [Source::Pregame(&keep), Source::Gameplay(&pass)];
    assert_eq!(
        build_checked_untrusted_visible_history_v1(
            "mismatched_exact",
            &mismatched_sources,
            &[
                MtgoVisibleHistoryLinkV1::StartOfCapturedHistory,
                MtgoVisibleHistoryLinkV1::ExactFrameMatch,
            ],
        )
        .err()
        .unwrap()
        .code(),
        "visible_history_exact_link_mismatch"
    );

    let land = play_land();
    let mana = mana();
    let exact_sources = [Source::VisibleObject(&land), Source::VisibleObject(&mana)];
    assert_eq!(
        build_checked_untrusted_visible_history_v1(
            "false_gap",
            &exact_sources,
            &[
                MtgoVisibleHistoryLinkV1::StartOfCapturedHistory,
                gap("claimed_gap"),
            ],
        )
        .err()
        .unwrap()
        .code(),
        "visible_history_gap_over_exact_frame"
    );
}

#[test]
fn start_and_gap_reason_declarations_are_strict() {
    let keep = keep();
    let pass = pass();
    let sources = [Source::Pregame(&keep), Source::Gameplay(&pass)];
    assert_eq!(
        build_checked_untrusted_visible_history_v1(
            "bad_start",
            &sources,
            &[gap("not_a_start"), gap("uncaptured_transition"),],
        )
        .err()
        .unwrap()
        .code(),
        "visible_history_first_link_invalid"
    );
    assert_eq!(
        build_checked_untrusted_visible_history_v1(
            "empty_gap_reason",
            &sources,
            &[
                MtgoVisibleHistoryLinkV1::StartOfCapturedHistory,
                MtgoVisibleHistoryLinkV1::VisibleGap {
                    reason_codes: vec![],
                },
            ],
        )
        .err()
        .unwrap()
        .code(),
        "visible_history_gap_reason_count_invalid"
    );
}

#[test]
fn source_count_commitment_and_client_geometry_fail_closed() {
    assert_eq!(
        build_checked_untrusted_visible_history_v1("empty", &[], &[])
            .err()
            .unwrap()
            .code(),
        "visible_history_source_count_invalid"
    );

    let keep = keep();
    assert_eq!(
        build_checked_untrusted_visible_history_v1("link_count", &[Source::Pregame(&keep)], &[],)
            .err()
            .unwrap()
            .code(),
        "visible_history_link_count_mismatch"
    );
    assert_eq!(
        build_checked_untrusted_visible_history_v1(
            "duplicate",
            &[Source::Pregame(&keep), Source::Pregame(&keep)],
            &[
                MtgoVisibleHistoryLinkV1::StartOfCapturedHistory,
                gap("duplicate_capture"),
            ],
        )
        .err()
        .unwrap()
        .code(),
        "visible_history_duplicate_transition"
    );

    let mut resized: MtgoGameplayCalibrationTraceV1 = serde_json::from_str(include_str!(
        "../fixtures/solitaire_pass_to_combat_transition_v1.json"
    ))
    .unwrap();
    resized.before_frame.client_size_px.width += 1;
    resized.after_frame.client_size_px.width += 1;
    let resized = validate_gameplay_calibration_trace_v1(resized).unwrap();
    assert_eq!(
        build_checked_untrusted_visible_history_v1(
            "geometry_drift",
            &[Source::Pregame(&keep), Source::Gameplay(&resized)],
            &[
                MtgoVisibleHistoryLinkV1::StartOfCapturedHistory,
                gap("capture_geometry_changed"),
            ],
        )
        .err()
        .unwrap()
        .code(),
        "visible_history_client_size_changed"
    );
}

#[test]
fn exact_single_trace_is_still_not_kernel_context_authority() {
    let keep = keep();
    let checked = build_checked_untrusted_visible_history_v1(
        "single_opening_action",
        &[Source::Pregame(&keep)],
        &[MtgoVisibleHistoryLinkV1::StartOfCapturedHistory],
    )
    .unwrap();
    assert!(checked.exact_pixel_chain_from_opening_hand());
    assert_eq!(checked.trailing_exact_step_count(), 1);
    assert!(!checked.safe_for_kernel_context());
}
