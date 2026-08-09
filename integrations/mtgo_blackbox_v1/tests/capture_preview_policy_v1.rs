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
        assert!(PREVIEW_SCRIPT.contains(required), "missing guard: {required}");
    }
}
