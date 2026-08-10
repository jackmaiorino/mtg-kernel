const CLICK_SCRIPT: &str = include_str!("../scripts/invoke_supervised_mtgo_click_v1.ps1");

#[test]
fn click_helper_is_exactly_one_left_click_with_no_keyboard_path() {
    for required in [
        "MOUSEEVENTF_LEFTDOWN",
        "MOUSEEVENTF_LEFTUP",
        "SendInput(2",
        "SendOneLeftClick()",
        "one_supervised_visible_left_click",
    ] {
        assert!(
            CLICK_SCRIPT.contains(required),
            "missing click primitive: {required}"
        );
    }

    let lower = CLICK_SCRIPT.to_ascii_lowercase();
    for forbidden in [
        "input_keyboard",
        "keybdinput",
        "keybd_event",
        "mouseeventf_right",
        "mouseeventf_middle",
        "mouseeventf_wheel",
        "postmessage",
        "sendmessage",
        "readprocessmemory",
        "writeprocessmemory",
        "createprocess",
        "virtualalloc",
        "setwindowlong",
        "uiautomationclient",
        "automationelement",
        "httpclient",
        "invoke-webrequest",
        "tcpclient",
        "udpclient",
    ] {
        assert!(
            !lower.contains(forbidden),
            "forbidden control marker: {forbidden}"
        );
    }
}

#[test]
fn click_helper_requires_live_identity_geometry_and_visible_hit_checks() {
    for required in [
        "ExpectedProcessId",
        "ExpectedProcessStartUtc",
        "ExpectedExecutableSha256",
        "ExpectedSignerThumbprint",
        "ExpectedSignerSubject",
        "ExpectedWindowTitle",
        "ExpectedClientWidth",
        "ExpectedClientHeight",
        "ExpectedDpi",
        "EnterPerMonitorV2",
        "Get-AuthenticodeSignature",
        "GetDpiForWindow",
        "GetForegroundWindow",
        "WindowFromPoint",
        "ClientToScreen",
        "GetCursorPos",
        "MTGO_CLICK_PRE_INPUT_STATE_CHANGED",
        "MTGO_CLICK_VISIBLE_HIT_TARGET_MISMATCH",
        "MTGO_CLICK_CURSOR_POSITION_CHANGED",
    ] {
        assert!(
            CLICK_SCRIPT.contains(required),
            "missing click guard: {required}"
        );
    }

    assert!(
        !CLICK_SCRIPT.contains("MTGO_CLICK_FOREGROUND_OWNED_WINDOW_IS_MAIN_CLIENT"),
        "a duel may become Process.MainWindowHandle and must be selected by exact foreground identity"
    );
}

#[test]
fn click_helper_does_not_claim_postcondition_or_autonomous_authority() {
    for required in [
        "postcondition_verified = $false",
        "safe_for_autonomous_input = $false",
        "safe_for_purchase = $false",
        "safe_for_queue_entry = $false",
    ] {
        assert!(
            CLICK_SCRIPT.contains(required),
            "missing nonclaim: {required}"
        );
    }
}
