use crate::probe::{
    confirm_pregame_keep_to_first_main_transition_v3, confirm_pregame_mulligan_transition_v3,
    prepare_pregame_actuation_v3, MtgoPlannedPregamePostconditionV3,
    OpaqueMtgoConfirmedKeepToFirstMainTransitionV3, OpaqueMtgoConfirmedMulliganTransitionV3,
    OpaqueMtgoDxgiFirstMainMeasurementV3, OpaqueMtgoDxgiMulliganMeasurementV3,
    OpaqueMtgoPregameActionPlanV3, PreparedPregameActuationV3,
};
use mtgo_blackbox_v1::{
    validate_authorization_for_mode_v1, MtgoAuthorizationScopeV1, MtgoPregameActionSemanticV1,
    MtgoRuntimeModeV1,
};
use sha2::{Digest, Sha256};
use std::ffi::c_void;
use std::mem::size_of;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, GetDpiForWindow, GetThreadDpiAwarenessContext,
    SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetClientRect, GetForegroundWindow, GetWindowThreadProcessId, IsHungAppWindow,
    IsIconic, IsWindow, IsWindowVisible, SetCursorPos, WindowFromPoint, GA_ROOT,
};

const PREGAME_INPUT_RECEIPT_DOMAIN_V3: &[u8] = b"mtgo-private-pregame-input-receipt-v3";
const PRIVATE_MATCH_AUTHORIZATION_DOMAIN_V3: &[u8] = b"mtgo-private-match-authorization-v3";
const RATIFIED_PRIVATE_MATCH_AUTHORIZATION_COMMITMENT_V3: Option<&str> = None;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoPregameInputGateStatusV3 {
    Idle,
    Preparing,
    AwaitingVisiblePostcondition,
    Halted,
}

enum PregameInputGateStateV3 {
    Idle,
    Preparing,
    AwaitingVisiblePostcondition { receipt_sha256: String },
    Halted,
}

static PREGAME_INPUT_GATE_V3: OnceLock<Mutex<PregameInputGateStateV3>> = OnceLock::new();

/// A separately ratified exact-account and exact-correspondence authorization
/// for visible-only private-match input. The production ratification list is
/// empty until the exact Daybreak correspondence artifact is reviewed and
/// committed.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::RatifiedMtgoPrivateMatchAuthorizationV3;
/// let _forged = RatifiedMtgoPrivateMatchAuthorizationV3 {};
/// ```
pub struct RatifiedMtgoPrivateMatchAuthorizationV3 {
    scope: MtgoAuthorizationScopeV1,
    visible_account_alias: String,
    authorization_commitment_sha256: String,
}

impl RatifiedMtgoPrivateMatchAuthorizationV3 {
    pub fn account_alias_sha256_v3(&self) -> &str {
        &self.scope.account_alias_sha256
    }

    pub fn written_permission_sha256_v3(&self) -> &str {
        &self.scope.written_permission_sha256
    }

    pub fn authorization_commitment_sha256_v3(&self) -> &str {
        &self.authorization_commitment_sha256
    }
}

/// Proof that one authorized private-match pregame click was emitted and that
/// the process-wide gate is waiting for its exact visible postcondition.
///
/// This value exposes no pixels, window handle, or coordinates. Dropping it
/// does not release the gate. A failed confirmation halts later input for the
/// life of the process.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPendingPregameInputV3;
/// let _forged = OpaqueMtgoPendingPregameInputV3 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPendingPregameInputV3;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoPendingPregameInputV3>();
/// ```
pub struct OpaqueMtgoPendingPregameInputV3 {
    plan: OpaqueMtgoPregameActionPlanV3,
    input_receipt_sha256: String,
    before_input_capture_commitment_sha256: String,
    input_sent_at_unix_millis: u128,
    cursor_parked_outside_client: bool,
}

impl OpaqueMtgoPendingPregameInputV3 {
    pub fn selected_semantic_v3(&self) -> &MtgoPregameActionSemanticV1 {
        self.plan.selected_semantic_v3()
    }

    pub fn planned_postcondition_v3(&self) -> &MtgoPlannedPregamePostconditionV3 {
        self.plan.planned_postcondition_v3()
    }

    pub fn action_plan_commitment_sha256_v3(&self) -> &str {
        self.plan.action_plan_commitment_sha256_v3()
    }

    pub fn before_input_capture_commitment_sha256_v3(&self) -> &str {
        &self.before_input_capture_commitment_sha256
    }

    pub fn input_receipt_sha256_v3(&self) -> &str {
        &self.input_receipt_sha256
    }

    pub fn input_sent_at_unix_millis_v3(&self) -> u128 {
        self.input_sent_at_unix_millis
    }

    pub fn cursor_parked_outside_client_v3(&self) -> bool {
        self.cursor_parked_outside_client
    }

    pub fn safe_for_next_input_v3(&self) -> bool {
        false
    }

    pub fn safe_for_purchase_v3(&self) -> bool {
        false
    }

    pub fn safe_for_queue_entry_v3(&self) -> bool {
        false
    }
}

pub fn pregame_input_gate_status_v3() -> Result<MtgoPregameInputGateStatusV3, String> {
    let gate = input_gate_v3()
        .lock()
        .map_err(|_| "the pregame input gate is poisoned".to_owned())?;
    Ok(match &*gate {
        PregameInputGateStateV3::Idle => MtgoPregameInputGateStatusV3::Idle,
        PregameInputGateStateV3::Preparing => MtgoPregameInputGateStatusV3::Preparing,
        PregameInputGateStateV3::AwaitingVisiblePostcondition { .. } => {
            MtgoPregameInputGateStatusV3::AwaitingVisiblePostcondition
        }
        PregameInputGateStateV3::Halted => MtgoPregameInputGateStatusV3::Halted,
    })
}

pub fn execute_authorized_private_match_pregame_action_v3(
    plan: OpaqueMtgoPregameActionPlanV3,
    authorization: RatifiedMtgoPrivateMatchAuthorizationV3,
) -> Result<OpaqueMtgoPendingPregameInputV3, String> {
    reserve_input_gate_v3()?;

    let prepared = match prepare_pregame_actuation_v3(&plan, &authorization.visible_account_alias) {
        Ok(prepared) => prepared,
        Err(error) => {
            release_unattempted_reservation_v3()?;
            return Err(error);
        }
    };
    let input_sent_at_unix_millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(error) => {
            release_unattempted_reservation_v3()?;
            return Err(format!("system clock is before epoch: {error}"));
        }
    };
    if validate_preinput_capture_freshness_v3(
        prepared.current_captured_at_unix_millis,
        input_sent_at_unix_millis,
    )
    .is_err()
    {
        release_unattempted_reservation_v3()?;
        return Err("the immediate pre-input capture is stale or from the future".to_owned());
    }
    halt_before_input_attempt_v3()?;
    let cursor_parked_outside_client = send_exactly_one_left_click_v3(&prepared)?;
    let input_receipt_sha256 = input_receipt_commitment_v3(
        &prepared,
        &authorization.scope,
        input_sent_at_unix_millis,
        cursor_parked_outside_client,
    );
    set_pending_v3(&input_receipt_sha256)?;

    Ok(OpaqueMtgoPendingPregameInputV3 {
        plan,
        input_receipt_sha256,
        before_input_capture_commitment_sha256: prepared.current_capture_commitment_sha256,
        input_sent_at_unix_millis,
        cursor_parked_outside_client,
    })
}

pub fn ratify_private_match_authorization_v3(
    scope: MtgoAuthorizationScopeV1,
    visible_account_alias: String,
) -> Result<RatifiedMtgoPrivateMatchAuthorizationV3, String> {
    ratify_private_match_authorization_with_commitment_v3(
        scope,
        visible_account_alias,
        RATIFIED_PRIVATE_MATCH_AUTHORIZATION_COMMITMENT_V3,
    )
}

fn ratify_private_match_authorization_with_commitment_v3(
    scope: MtgoAuthorizationScopeV1,
    visible_account_alias: String,
    ratified_commitment_sha256: Option<&str>,
) -> Result<RatifiedMtgoPrivateMatchAuthorizationV3, String> {
    validate_private_match_authorization_v3(&scope, &visible_account_alias)?;
    if scope.shadow_observation
        || scope.open_play_input
        || scope.league_input
        || scope.challenge_input
        || scope.other_prize_event_input
    {
        return Err(
            "the private-match actuator requires a scope with every other runtime mode disabled"
                .to_owned(),
        );
    }
    let authorization_commitment_sha256 =
        private_match_authorization_commitment_v3(&scope, &visible_account_alias);
    if ratified_commitment_sha256 != Some(authorization_commitment_sha256.as_str()) {
        return Err(
            "the exact private-match permission correspondence is not ratified in this build"
                .to_owned(),
        );
    }
    Ok(RatifiedMtgoPrivateMatchAuthorizationV3 {
        scope,
        visible_account_alias,
        authorization_commitment_sha256,
    })
}

pub fn confirm_pending_pregame_mulligan_v3(
    pending: OpaqueMtgoPendingPregameInputV3,
    after: OpaqueMtgoDxgiMulliganMeasurementV3,
) -> Result<OpaqueMtgoConfirmedMulliganTransitionV3, String> {
    require_matching_pending_v3(&pending.input_receipt_sha256)?;
    match confirm_pregame_mulligan_transition_v3(pending.plan, after) {
        Ok(confirmed) => {
            release_confirmed_pending_v3(&pending.input_receipt_sha256)?;
            Ok(confirmed)
        }
        Err(error) => {
            halt_gate_v3()?;
            Err(format!(
                "pregame Mulligan postcondition failed and the input gate is halted: {error}"
            ))
        }
    }
}

pub fn confirm_pending_pregame_keep_to_first_main_v3(
    pending: OpaqueMtgoPendingPregameInputV3,
    after: OpaqueMtgoDxgiFirstMainMeasurementV3,
) -> Result<OpaqueMtgoConfirmedKeepToFirstMainTransitionV3, String> {
    require_matching_pending_v3(&pending.input_receipt_sha256)?;
    match confirm_pregame_keep_to_first_main_transition_v3(pending.plan, after) {
        Ok(confirmed) => {
            release_confirmed_pending_v3(&pending.input_receipt_sha256)?;
            Ok(confirmed)
        }
        Err(error) => {
            halt_gate_v3()?;
            Err(format!(
                "pregame Keep postcondition failed and the input gate is halted: {error}"
            ))
        }
    }
}

fn validate_private_match_authorization_v3(
    authorization: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
) -> Result<(), String> {
    validate_authorization_for_mode_v1(authorization, MtgoRuntimeModeV1::PrivateMatchInput)
        .map_err(|error| format!("private-match authorization rejected: {error}"))?;
    if visible_account_alias.is_empty()
        || visible_account_alias.len() > 64
        || visible_account_alias.chars().any(char::is_control)
        || format!("{:x}", Sha256::digest(visible_account_alias.as_bytes()))
            != authorization.account_alias_sha256
    {
        return Err("the visible account alias does not match the authorized account".to_owned());
    }
    Ok(())
}

fn private_match_authorization_commitment_v3(
    authorization: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(PRIVATE_MATCH_AUTHORIZATION_DOMAIN_V3);
    let schema_version = authorization.schema_version.to_be_bytes();
    for part in [
        schema_version.as_slice(),
        authorization.account_alias_sha256.as_bytes(),
        authorization.written_permission_sha256.as_bytes(),
        visible_account_alias.as_bytes(),
    ] {
        update_hash_part_v3(&mut hasher, part);
    }
    for flag in [
        authorization.visible_channels_only,
        authorization.shadow_observation,
        authorization.private_match_input,
        authorization.open_play_input,
        authorization.league_input,
        authorization.challenge_input,
        authorization.other_prize_event_input,
    ] {
        update_hash_part_v3(&mut hasher, &[u8::from(flag)]);
    }
    format!("{:x}", hasher.finalize())
}

fn input_gate_v3() -> &'static Mutex<PregameInputGateStateV3> {
    PREGAME_INPUT_GATE_V3.get_or_init(|| Mutex::new(PregameInputGateStateV3::Idle))
}

fn validate_preinput_capture_freshness_v3(
    captured_at_unix_millis: u128,
    input_at_unix_millis: u128,
) -> Result<(), String> {
    if input_at_unix_millis < captured_at_unix_millis
        || input_at_unix_millis - captured_at_unix_millis > 2_000
    {
        return Err("the immediate pre-input capture is stale or from the future".to_owned());
    }
    Ok(())
}

fn reserve_input_gate_v3() -> Result<(), String> {
    let mut gate = input_gate_v3()
        .lock()
        .map_err(|_| "the pregame input gate is poisoned".to_owned())?;
    match &*gate {
        PregameInputGateStateV3::Idle => {
            *gate = PregameInputGateStateV3::Preparing;
            Ok(())
        }
        PregameInputGateStateV3::Preparing => {
            Err("another pregame input is already being prepared".to_owned())
        }
        PregameInputGateStateV3::AwaitingVisiblePostcondition { .. } => {
            Err("a visible postcondition is still pending".to_owned())
        }
        PregameInputGateStateV3::Halted => {
            Err("the pregame input gate is halted for this process".to_owned())
        }
    }
}

fn release_unattempted_reservation_v3() -> Result<(), String> {
    let mut gate = input_gate_v3()
        .lock()
        .map_err(|_| "the pregame input gate is poisoned".to_owned())?;
    if !matches!(*gate, PregameInputGateStateV3::Preparing) {
        *gate = PregameInputGateStateV3::Halted;
        return Err("the pregame input reservation changed unexpectedly".to_owned());
    }
    *gate = PregameInputGateStateV3::Idle;
    Ok(())
}

fn halt_before_input_attempt_v3() -> Result<(), String> {
    let mut gate = input_gate_v3()
        .lock()
        .map_err(|_| "the pregame input gate is poisoned".to_owned())?;
    if !matches!(*gate, PregameInputGateStateV3::Preparing) {
        *gate = PregameInputGateStateV3::Halted;
        return Err("the pregame input reservation changed unexpectedly".to_owned());
    }
    *gate = PregameInputGateStateV3::Halted;
    Ok(())
}

fn set_pending_v3(receipt_sha256: &str) -> Result<(), String> {
    let mut gate = input_gate_v3()
        .lock()
        .map_err(|_| "the pregame input gate is poisoned".to_owned())?;
    if !matches!(*gate, PregameInputGateStateV3::Halted) {
        *gate = PregameInputGateStateV3::Halted;
        return Err("the pregame input gate was not armed for an input attempt".to_owned());
    }
    *gate = PregameInputGateStateV3::AwaitingVisiblePostcondition {
        receipt_sha256: receipt_sha256.to_owned(),
    };
    Ok(())
}

fn require_matching_pending_v3(receipt_sha256: &str) -> Result<(), String> {
    let gate = input_gate_v3()
        .lock()
        .map_err(|_| "the pregame input gate is poisoned".to_owned())?;
    match &*gate {
        PregameInputGateStateV3::AwaitingVisiblePostcondition {
            receipt_sha256: expected,
        } if expected == receipt_sha256 => Ok(()),
        PregameInputGateStateV3::AwaitingVisiblePostcondition { .. } => {
            Err("the pending input receipt does not match the process gate".to_owned())
        }
        _ => Err("the process gate is not awaiting this postcondition".to_owned()),
    }
}

fn release_confirmed_pending_v3(receipt_sha256: &str) -> Result<(), String> {
    let mut gate = input_gate_v3()
        .lock()
        .map_err(|_| "the pregame input gate is poisoned".to_owned())?;
    match &*gate {
        PregameInputGateStateV3::AwaitingVisiblePostcondition {
            receipt_sha256: expected,
        } if expected == receipt_sha256 => {
            *gate = PregameInputGateStateV3::Idle;
            Ok(())
        }
        _ => {
            *gate = PregameInputGateStateV3::Halted;
            Err("the confirmed input receipt does not match the process gate".to_owned())
        }
    }
}

fn halt_gate_v3() -> Result<(), String> {
    let mut gate = input_gate_v3()
        .lock()
        .map_err(|_| "the pregame input gate is poisoned".to_owned())?;
    *gate = PregameInputGateStateV3::Halted;
    Ok(())
}

fn send_exactly_one_left_click_v3(prepared: &PreparedPregameActuationV3) -> Result<bool, String> {
    let previous_context =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if previous_context.is_invalid() {
        return Err("the input thread could not enter Per-Monitor V2 DPI awareness".to_owned());
    }
    let _dpi_guard = ActuatorDpiGuardV3(previous_context);
    if !unsafe {
        AreDpiAwarenessContextsEqual(
            GetThreadDpiAwarenessContext(),
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
    }
    .as_bool()
    {
        return Err("the input thread is not Per-Monitor V2 DPI aware".to_owned());
    }

    verify_live_target_v3(prepared, false)?;
    let mut cursor_park_guard = CursorParkGuardV3 {
        x: prepared.park_x_desktop_px,
        y: prepared.park_y_desktop_px,
        parked: false,
    };
    unsafe { SetCursorPos(prepared.target_x_desktop_px, prepared.target_y_desktop_px) }
        .map_err(|error| format!("move cursor to selected control: {error}"))?;
    verify_live_target_v3(prepared, true)?;

    let inputs = [
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: MOUSEEVENTF_LEFTDOWN,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: MOUSEEVENTF_LEFTUP,
                    ..Default::default()
                },
            },
        },
    ];
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent != 2 {
        if sent == 1 {
            let release = [INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dwFlags: MOUSEEVENTF_LEFTUP,
                        ..Default::default()
                    },
                },
            }];
            let _ = unsafe { SendInput(&release, size_of::<INPUT>() as i32) };
        }
        return Err(format!(
            "SendInput emitted {sent} of 2 required mouse records"
        ));
    }
    Ok(cursor_park_guard.park_now())
}

struct ActuatorDpiGuardV3(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT);

impl Drop for ActuatorDpiGuardV3 {
    fn drop(&mut self) {
        unsafe {
            let _ = SetThreadDpiAwarenessContext(self.0);
        }
    }
}

struct CursorParkGuardV3 {
    x: i32,
    y: i32,
    parked: bool,
}

impl CursorParkGuardV3 {
    fn park_now(&mut self) -> bool {
        self.parked = unsafe { SetCursorPos(self.x, self.y) }.is_ok();
        self.parked
    }
}

impl Drop for CursorParkGuardV3 {
    fn drop(&mut self) {
        if !self.parked {
            let _ = unsafe { SetCursorPos(self.x, self.y) };
        }
    }
}

fn verify_live_target_v3(
    prepared: &PreparedPregameActuationV3,
    require_point_hit_test: bool,
) -> Result<(), String> {
    let hwnd = HWND(prepared.hwnd as *mut c_void);
    if !unsafe { IsWindow(Some(hwnd)) }.as_bool()
        || !unsafe { IsWindowVisible(hwnd) }.as_bool()
        || unsafe { IsIconic(hwnd) }.as_bool()
        || unsafe { IsHungAppWindow(hwnd) }.as_bool()
        || unsafe { GetForegroundWindow() } != hwnd
        || unsafe { GetAncestor(hwnd, GA_ROOT) } != hwnd
    {
        return Err("the authorized MTGO window lost foreground admission".to_owned());
    }
    let mut process_id = 0_u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if process_id != prepared.process_id || unsafe { GetDpiForWindow(hwnd) } != prepared.dpi {
        return Err("the authorized MTGO process or DPI changed before input".to_owned());
    }
    if live_process_start_filetime_v3(prepared.process_id)? != prepared.process_start_filetime_100ns
    {
        return Err("the authorized MTGO process incarnation changed before input".to_owned());
    }

    let mut client = RECT::default();
    unsafe { GetClientRect(hwnd, &mut client) }
        .map_err(|error| format!("read MTGO client bounds before input: {error}"))?;
    let mut origin = POINT {
        x: client.left,
        y: client.top,
    };
    if !unsafe { ClientToScreen(hwnd, &mut origin) }.as_bool() {
        return Err("map MTGO client bounds before input failed".to_owned());
    }
    let right = origin
        .x
        .checked_add(client.right - client.left)
        .ok_or("current MTGO client right overflow")?;
    let bottom = origin
        .y
        .checked_add(client.bottom - client.top)
        .ok_or("current MTGO client bottom overflow")?;
    if prepared.client_rect_desktop_px.left != origin.x
        || prepared.client_rect_desktop_px.top != origin.y
        || prepared.client_rect_desktop_px.right != right
        || prepared.client_rect_desktop_px.bottom != bottom
    {
        return Err("the MTGO client geometry changed before input".to_owned());
    }

    if require_point_hit_test {
        let hit = unsafe {
            WindowFromPoint(POINT {
                x: prepared.target_x_desktop_px,
                y: prepared.target_y_desktop_px,
            })
        };
        if hit.0.is_null() || unsafe { GetAncestor(hit, GA_ROOT) } != hwnd {
            return Err("another window owns the selected control point".to_owned());
        }
        let mut hit_process_id = 0_u32;
        unsafe { GetWindowThreadProcessId(hit, Some(&mut hit_process_id)) };
        if hit_process_id != prepared.process_id {
            return Err("the selected control point is not owned by MTGO".to_owned());
        }
    }
    Ok(())
}

struct ActuatorProcessHandleV3(HANDLE);

impl Drop for ActuatorProcessHandleV3 {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn live_process_start_filetime_v3(process_id: u32) -> Result<u64, String> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .map_err(|error| format!("open MTGO process before input: {error}"))?;
    let handle = ActuatorProcessHandleV3(handle);
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe { GetProcessTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user) }
        .map_err(|error| format!("read MTGO process start before input: {error}"))?;
    Ok(((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64)
}

fn input_receipt_commitment_v3(
    prepared: &PreparedPregameActuationV3,
    authorization: &MtgoAuthorizationScopeV1,
    input_sent_at_unix_millis: u128,
    cursor_parked_outside_client: bool,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(PREGAME_INPUT_RECEIPT_DOMAIN_V3);
    for part in [
        authorization.account_alias_sha256.as_bytes(),
        authorization.written_permission_sha256.as_bytes(),
        prepared.action_plan_commitment_sha256.as_bytes(),
        prepared.current_capture_commitment_sha256.as_bytes(),
    ] {
        update_hash_part_v3(&mut hasher, part);
    }
    update_hash_part_v3(
        &mut hasher,
        semantic_bytes_v3(&prepared.selected_semantic).as_bytes(),
    );
    update_hash_part_v3(
        &mut hasher,
        postcondition_bytes_v3(&prepared.planned_postcondition).as_bytes(),
    );
    for coordinate in [
        prepared.target_x_desktop_px,
        prepared.target_y_desktop_px,
        prepared.park_x_desktop_px,
        prepared.park_y_desktop_px,
    ] {
        update_hash_part_v3(&mut hasher, &coordinate.to_be_bytes());
    }
    update_hash_part_v3(&mut hasher, &input_sent_at_unix_millis.to_be_bytes());
    update_hash_part_v3(&mut hasher, &[u8::from(cursor_parked_outside_client)]);
    format!("{:x}", hasher.finalize())
}

fn semantic_bytes_v3(semantic: &MtgoPregameActionSemanticV1) -> String {
    match semantic {
        MtgoPregameActionSemanticV1::Mulligan { next_hand_size } => {
            format!("mulligan:{next_hand_size}")
        }
        MtgoPregameActionSemanticV1::KeepOpeningHand => "keep_opening_hand".to_owned(),
    }
}

fn postcondition_bytes_v3(postcondition: &MtgoPlannedPregamePostconditionV3) -> String {
    match postcondition {
        MtgoPlannedPregamePostconditionV3::NextMulliganPrompt {
            prospective_keep_size,
        } => format!("next_mulligan_prompt:{prospective_keep_size}"),
        MtgoPlannedPregamePostconditionV3::LondonBottoming {
            required_bottom_count,
        } => format!("london_bottoming:{required_bottom_count}"),
        MtgoPlannedPregamePostconditionV3::GameplayFirstMain => "gameplay_first_main".to_owned(),
    }
}

fn update_hash_part_v3(hasher: &mut Sha256, part: &[u8]) {
    hasher.update((part.len() as u64).to_be_bytes());
    hasher.update(part);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authorized_scope_v3(alias: &str) -> MtgoAuthorizationScopeV1 {
        MtgoAuthorizationScopeV1 {
            account_alias_sha256: format!("{:x}", Sha256::digest(alias.as_bytes())),
            written_permission_sha256: "a".repeat(64),
            private_match_input: true,
            ..MtgoAuthorizationScopeV1::default()
        }
    }

    #[test]
    fn private_match_authorization_binds_the_visible_account() {
        let scope = authorized_scope_v3("UnbuckledPie");
        validate_private_match_authorization_v3(&scope, "UnbuckledPie").unwrap();
        assert!(validate_private_match_authorization_v3(&scope, "another-account").is_err());

        let mut no_private_input = scope.clone();
        no_private_input.private_match_input = false;
        assert!(
            validate_private_match_authorization_v3(&no_private_input, "UnbuckledPie").is_err()
        );

        let mut hidden = scope;
        hidden.visible_channels_only = false;
        assert!(validate_private_match_authorization_v3(&hidden, "UnbuckledPie").is_err());
    }

    #[test]
    fn production_permission_ratification_is_empty_and_scope_exact() {
        let scope = authorized_scope_v3("UnbuckledPie");
        assert!(
            ratify_private_match_authorization_v3(scope.clone(), "UnbuckledPie".to_owned())
                .is_err()
        );

        let baseline = private_match_authorization_commitment_v3(&scope, "UnbuckledPie");
        let mut broader = scope.clone();
        broader.league_input = true;
        assert_ne!(
            baseline,
            private_match_authorization_commitment_v3(&broader, "UnbuckledPie")
        );
        assert!(ratify_private_match_authorization_v3(broader, "UnbuckledPie".to_owned()).is_err());

        let expected = private_match_authorization_commitment_v3(&scope, "UnbuckledPie");
        let ratified = ratify_private_match_authorization_with_commitment_v3(
            scope,
            "UnbuckledPie".to_owned(),
            Some(&expected),
        )
        .unwrap();
        assert_eq!(ratified.authorization_commitment_sha256_v3(), expected);
    }

    #[test]
    fn cursor_park_point_must_be_on_output_and_outside_client() {
        let client = crate::SignedRectV1 {
            left: 200,
            top: 100,
            right: 1_750,
            bottom: 1_025,
        };
        let output = crate::SignedRectV1 {
            left: -100,
            top: -50,
            right: 2_460,
            bottom: 1_390,
        };
        let point = crate::probe::choose_cursor_park_point_v3(&client, &output).unwrap();
        assert!(output.contains_point(point.0, point.1));
        assert!(!client.contains_point(point.0, point.1));

        assert!(crate::probe::choose_cursor_park_point_v3(&output, &output).is_err());
    }

    #[test]
    fn freshness_and_process_gate_block_stale_or_concurrent_input() {
        validate_preinput_capture_freshness_v3(1_000, 1_000).unwrap();
        validate_preinput_capture_freshness_v3(1_000, 3_000).unwrap();
        assert!(validate_preinput_capture_freshness_v3(1_000, 3_001).is_err());
        assert!(validate_preinput_capture_freshness_v3(1_001, 1_000).is_err());

        {
            let mut gate = input_gate_v3().lock().unwrap();
            *gate = PregameInputGateStateV3::Idle;
        }
        reserve_input_gate_v3().unwrap();
        assert_eq!(
            pregame_input_gate_status_v3().unwrap(),
            MtgoPregameInputGateStatusV3::Preparing
        );
        assert!(reserve_input_gate_v3().is_err());
        halt_before_input_attempt_v3().unwrap();
        set_pending_v3(&"a".repeat(64)).unwrap();
        assert_eq!(
            pregame_input_gate_status_v3().unwrap(),
            MtgoPregameInputGateStatusV3::AwaitingVisiblePostcondition
        );
        assert!(reserve_input_gate_v3().is_err());
        assert!(require_matching_pending_v3(&"b".repeat(64)).is_err());
        require_matching_pending_v3(&"a".repeat(64)).unwrap();
        release_confirmed_pending_v3(&"a".repeat(64)).unwrap();
        assert_eq!(
            pregame_input_gate_status_v3().unwrap(),
            MtgoPregameInputGateStatusV3::Idle
        );
    }

    #[test]
    fn input_receipt_binds_authorization_plan_frame_action_point_and_time() {
        let authorization = authorized_scope_v3("UnbuckledPie");
        let mut prepared = PreparedPregameActuationV3 {
            hwnd: 10,
            process_id: 20,
            process_start_filetime_100ns: 30,
            dpi: 120,
            client_rect_desktop_px: crate::SignedRectV1 {
                left: 100,
                top: 200,
                right: 1_650,
                bottom: 1_125,
            },
            target_x_desktop_px: 130,
            target_y_desktop_px: 353,
            park_x_desktop_px: 1,
            park_y_desktop_px: 1,
            current_capture_commitment_sha256: "b".repeat(64),
            current_captured_at_unix_millis: 1_000,
            action_plan_commitment_sha256: "c".repeat(64),
            selected_semantic: MtgoPregameActionSemanticV1::Mulligan { next_hand_size: 6 },
            planned_postcondition: MtgoPlannedPregamePostconditionV3::NextMulliganPrompt {
                prospective_keep_size: 6,
            },
        };
        let baseline = input_receipt_commitment_v3(&prepared, &authorization, 1_100, true);

        prepared.target_x_desktop_px += 1;
        assert_ne!(
            baseline,
            input_receipt_commitment_v3(&prepared, &authorization, 1_100, true)
        );
        prepared.target_x_desktop_px -= 1;
        prepared.selected_semantic = MtgoPregameActionSemanticV1::KeepOpeningHand;
        assert_ne!(
            baseline,
            input_receipt_commitment_v3(&prepared, &authorization, 1_100, true)
        );
        prepared.selected_semantic = MtgoPregameActionSemanticV1::Mulligan { next_hand_size: 6 };
        prepared.current_capture_commitment_sha256 = "d".repeat(64);
        assert_ne!(
            baseline,
            input_receipt_commitment_v3(&prepared, &authorization, 1_100, true)
        );
        prepared.current_capture_commitment_sha256 = "b".repeat(64);
        assert_ne!(
            baseline,
            input_receipt_commitment_v3(&prepared, &authorization, 1_101, true)
        );
        assert_ne!(
            baseline,
            input_receipt_commitment_v3(&prepared, &authorization, 1_100, false)
        );

        let mut different_permission = authorization;
        different_permission.written_permission_sha256 = "e".repeat(64);
        assert_ne!(
            baseline,
            input_receipt_commitment_v3(&prepared, &different_permission, 1_100, true)
        );
    }
}
