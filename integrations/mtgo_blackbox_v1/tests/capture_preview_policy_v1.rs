const PREVIEW_SCRIPT: &str = include_str!("../scripts/capture_visible_mtgo_preview_v1.ps1");

#[test]
fn preview_is_visibly_composed_and_non_actionable() {
    assert!(PREVIEW_SCRIPT.contains("CopyFromScreen"));
    assert!(PREVIEW_SCRIPT.contains("CopyPixelOperation]::SourceCopy"));
    assert!(!PREVIEW_SCRIPT.contains("CopyPixelOperation]::CaptureBlt"));
    assert!(PREVIEW_SCRIPT.contains("pending_visual_review"));
    assert!(PREVIEW_SCRIPT.contains("safe_for_semantic_evidence = $false"));
    assert!(PREVIEW_SCRIPT.contains("safe_for_ocr = $false"));
    assert!(PREVIEW_SCRIPT.contains("safe_for_policy_scoring = $false"));
    assert!(PREVIEW_SCRIPT.contains("safe_for_input = $false"));
}

#[test]
fn preview_source_excludes_hidden_state_and_control_apis() {
    let lower = PREVIEW_SCRIPT.to_ascii_lowercase();
    for forbidden in [
        "printwindow",
        "wm_print",
        "getwindowdc",
        "dwmregisterthumbnail",
        "readprocessmemory",
        "writeprocessmemory",
        "openprocess",
        "sendinput",
        "setforegroundwindow",
        "setcursorpos",
        "mouse_event",
        "keybd_event",
        "uiautomationclient",
        "automationelement",
        "httpclient",
        "invoke-webrequest",
        "tcpclient",
        "udpclient",
        "get-childitem",
        "get-content",
    ] {
        assert!(
            !lower.contains(forbidden),
            "forbidden capture or control API marker present: {forbidden}"
        );
    }
}

#[test]
fn preview_requires_identity_focus_geometry_and_post_capture_recheck() {
    for required in [
        "ExpectedProductVersion",
        "ExpectedExecutableSha256",
        "ExpectedSignerThumbprint",
        "ExpectedSignerSubject",
        "ExpectedDpi",
        "EnterPerMonitorV2",
        "VisibleTopLevelWindowsForProcess",
        "GetForegroundWindow",
        "GetWindowDisplayAffinity",
        "ClientBounds",
        "Monitors",
        "AuditOcclusion",
        "Assert-SnapshotsMatch",
        "MTGO_PREVIEW_OUTPUT_ALREADY_EXISTS",
        "MTGO_PREVIEW_OUTPUT_INSIDE_REPOSITORY_FORBIDDEN",
        "MTGO_PREVIEW_PARTIAL_CLEANUP_PATH_REJECTED",
        "Directory]::Delete($partialOutputPath, $true)",
    ] {
        assert!(
            PREVIEW_SCRIPT.contains(required),
            "missing guard: {required}"
        );
    }
}

#[test]
fn gameplay_preview_selects_only_a_visible_foreground_mtgo_duel() {
    for required in [
        "ForegroundSpectatorGame",
        "ExpectedGameFormat",
        "MTGO_PREVIEW_FOREGROUND_GAME_NOT_OWNED_BY_MTGO",
        "MTGO_PREVIEW_FOREGROUND_GAME_IS_MAIN_CLIENT",
        "MTGO_PREVIEW_FOREGROUND_GAME_NOT_IN_VISIBLE_WINDOW_SET",
        "MTGO_PREVIEW_GAME_WINDOW_TITLE_MISMATCH",
        "MTGO_PREVIEW_EXPECTED_EXACTLY_ONE_MAIN_CLIENT_WINDOW",
        "visible_mtgo_top_level_window_count",
        "visible_mtgo_top_level_window_set_sha256",
        "capture_role = $captureRole",
        "game_window_title_identity",
        "participants_and_match_game_ids",
        "participants_only",
        "mtgo_visible_spectator_gameplay_calibration_preview_v1",
    ] {
        assert!(
            PREVIEW_SCRIPT.contains(required),
            "missing gameplay preview guard: {required}"
        );
    }

    assert!(PREVIEW_SCRIPT.contains("^\\(1-on-1\\): {0}: Vs\\."));
    assert!(PREVIEW_SCRIPT.contains("[^,#\\r\\n]+$"));
    assert!(PREVIEW_SCRIPT.contains("Match #\\s*\\d+"));
    assert!(PREVIEW_SCRIPT.contains("Game #\\s*\\d+"));
}

#[test]
fn solitaire_preview_has_a_distinct_acting_player_scope() {
    for required in [
        "ForegroundSolitaireGame",
        "acting_player_solitaire",
        "^\\(Solitaire\\): {0}: Vs\\.",
        "mtgo_visible_solitaire_gameplay_calibration_preview_v1",
    ] {
        assert!(
            PREVIEW_SCRIPT.contains(required),
            "missing solitaire preview guard: {required}"
        );
    }
}

#[test]
fn dialog_preview_is_navigation_only_and_mtgo_owned() {
    for required in [
        "ForegroundOwnedDialog",
        "navigation_dialog",
        "MTGO_PREVIEW_DIALOG_FORBIDS_EXPECTED_GAME_FORMAT",
        "MTGO_PREVIEW_DIALOG_REQUIRES_MAIN_AND_DIALOG_WINDOWS",
        "MTGO_PREVIEW_FOREGROUND_DIALOG_NOT_OWNED_BY_MTGO",
        "MTGO_PREVIEW_FOREGROUND_DIALOG_NOT_IN_VISIBLE_WINDOW_SET",
        "MTGO_PREVIEW_DIALOG_WINDOW_TITLE_INVALID",
        "mtgo_visible_navigation_dialog_inspection_preview_v1",
    ] {
        assert!(
            PREVIEW_SCRIPT.contains(required),
            "missing navigation-dialog preview guard: {required}"
        );
    }
    assert!(
        !PREVIEW_SCRIPT.contains("MTGO_PREVIEW_FOREGROUND_DIALOG_IS_MAIN_CLIENT"),
        "a modal dialog may become Process.MainWindowHandle and remains identified by exact foreground ownership and title"
    );
}
