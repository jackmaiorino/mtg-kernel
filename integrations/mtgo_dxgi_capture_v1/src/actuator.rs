use crate::probe::{
    confirm_opaque_competitive_duel_pass_postcondition_v1,
    confirm_pregame_keep_to_bottom_six_transition_v3,
    confirm_pregame_keep_to_first_main_transition_v3, confirm_pregame_mulligan_transition_v3,
    prepare_pregame_actuation_v3, MtgoOpaqueCompetitiveDuelPassConfirmationCommitmentsV1,
    MtgoOpaqueCompetitiveDuelPassPreparationCommitmentsV1, MtgoPlannedPregamePostconditionV3,
    OpaqueMtgoConfirmedCompetitiveDuelPassV1, OpaqueMtgoConfirmedKeepToBottomSixTransitionV3,
    OpaqueMtgoConfirmedKeepToFirstMainTransitionV3, OpaqueMtgoConfirmedMulliganTransitionV3,
    OpaqueMtgoDxgiBottomSixInitialMeasurementV3, OpaqueMtgoDxgiFirstMainMeasurementV3,
    OpaqueMtgoDxgiMulliganMeasurementV3, OpaqueMtgoPregameActionPlanV3,
    OpaqueMtgoCompetitiveLaunchIdentityV1, OpaqueMtgoPreparedCompetitiveDuelPassV1,
    PreparedPregameActuationV3,
};
use mtgo_blackbox_v1::{
    competitive_match_gameplay_authorization_commitment_v1,
    competitive_mode_authorization_commitment_v1, validate_authorization_for_mode_v1,
    AdmittedMtgoDuelPerceptionProfileV1, CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    MtgoAuthorizationScopeV1, MtgoCompetitiveEventKindV1,
    MtgoCompetitiveMatchGameplayAuthorizationV1, MtgoPregameActionSemanticV1, MtgoRuntimeModeV1,
    MTGO_COMPETITIVE_MATCH_GAMEPLAY_AUTHORIZATION_SCHEMA_V1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::c_void;
use std::io::{self, IsTerminal, Write};
use std::mem::size_of;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
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
const COMPETITIVE_DUEL_PASS_INPUT_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-duel-priority-pass-input-receipt-v1";
const COMPETITIVE_DUEL_PASS_TRANSITION_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-duel-priority-pass-transition-receipt-v1";
const PRIVATE_MATCH_AUTHORIZATION_DOMAIN_V3: &[u8] = b"mtgo-private-match-authorization-v3";
const RATIFIED_PRIVATE_MATCH_AUTHORIZATION_COMMITMENT_V3: Option<&str> = None;
const COMPETITIVE_DUEL_PASS_AUTHORIZATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-duel-priority-pass-authorization-v1";
const COMPETITIVE_DUEL_PASS_AUTHORIZATION_FROM_REVIEW_DOMAIN_V2: &[u8] =
    b"mtgo-competitive-duel-priority-pass-authorization-from-review-v2";
const COMPETITIVE_DUEL_PASS_AUTHORIZATION_BINDING_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-duel-priority-pass-authorization-binding-v1";
const RATIFIED_COMPETITIVE_DUEL_PASS_AUTHORIZATION_COMMITMENT_V1: Option<&str> = None;
const RATIFIED_COMPETITIVE_DUEL_PASS_AUTHORIZATION_FROM_REVIEW_COMMITMENT_V2: Option<&str> = None;
const COMPETITIVE_MATCH_LAUNCH_AUTHORIZATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-match-launch-authorization-v1";
const RATIFIED_COMPETITIVE_MATCH_LAUNCH_AUTHORIZATION_COMMITMENT_V1: Option<&str> = None;
const COMPETITIVE_GAME_SESSION_INITIAL_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-game-session-initial-v1";
const COMPETITIVE_GAME_SESSION_ADVANCE_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-game-session-advance-v1";
const ATTENDED_COMPETITIVE_MATCH_LAUNCH_REQUEST_DOMAIN_V4: &[u8] =
    b"mtgo-attended-competitive-match-launch-request-v4";
const ATTENDED_COMPETITIVE_MATCH_LAUNCH_RECEIPT_DOMAIN_V4: &[u8] =
    b"mtgo-attended-competitive-match-launch-receipt-v4";
const ATTENDED_COMPETITIVE_MATCH_MAX_FRAME_ADVANCE_V4: u64 = 512;

const MTGO_ATTENDED_COMPETITIVE_MATCH_LAUNCH_REQUEST_SCHEMA_V4: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoPregameInputGateStatusV3 {
    Idle,
    Preparing,
    AwaitingVisiblePostcondition,
    Halted,
}

pub type MtgoInputGateStatusV3 = MtgoPregameInputGateStatusV3;

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

/// One separately ratified exact-account authorization for priority Pass in
/// exactly one of League or Challenge gameplay. Production ratification is
/// empty until the correspondence bytes and visible account identity are
/// reviewed and their exact commitment is compiled into the actuator.
///
/// This value grants no event-entry or purchase authority. It is not itself an
/// input command and has no coordinate conversion.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::RatifiedMtgoCompetitiveDuelPassAuthorizationV1;
/// let _forged = RatifiedMtgoCompetitiveDuelPassAuthorizationV1 {};
/// ```
pub struct RatifiedMtgoCompetitiveDuelPassAuthorizationV1 {
    scope: MtgoAuthorizationScopeV1,
    _permission_correspondence: Option<CheckedUntrustedMtgoAuthorizationCorrespondenceV1>,
    #[allow(dead_code)]
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
    mode_authorization_commitment_sha256: String,
    authorization_commitment_sha256: String,
    permission_review_commitment_sha256: Option<String>,
}

impl RatifiedMtgoCompetitiveDuelPassAuthorizationV1 {
    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.event_kind
    }

    pub fn account_alias_sha256_v1(&self) -> &str {
        &self.scope.account_alias_sha256
    }

    pub fn written_permission_sha256_v1(&self) -> &str {
        &self.scope.written_permission_sha256
    }

    pub fn mode_authorization_commitment_sha256_v1(&self) -> &str {
        &self.mode_authorization_commitment_sha256
    }

    pub fn authorization_commitment_sha256_v1(&self) -> &str {
        &self.authorization_commitment_sha256
    }

    pub fn permission_review_commitment_sha256_v2(&self) -> Option<&str> {
        self.permission_review_commitment_sha256.as_deref()
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoReviewedCompetitivePassRatificationCandidateV2 {
    pub permission_review_commitment_sha256: String,
    pub account_alias_sha256: String,
    pub correspondence_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub ratification_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
}

impl MtgoReviewedCompetitivePassRatificationCandidateV2 {
    pub fn safe_for_live_input_v2(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v2(&self) -> bool {
        false
    }

    pub fn permits_spending_v2(&self) -> bool {
        false
    }
}

/// A separately ratified owner launch for one exact competitive event, match,
/// game, entry record, and gameplay-authorization lifetime. Its production
/// trust root is independent from general Daybreak mode permission.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::RatifiedMtgoCompetitiveMatchLaunchV1;
/// let _forged = RatifiedMtgoCompetitiveMatchLaunchV1 {};
/// ```
pub struct RatifiedMtgoCompetitiveMatchLaunchV1 {
    #[allow(dead_code)]
    authorization: MtgoCompetitiveMatchGameplayAuthorizationV1,
    mode_authorization_commitment_sha256: String,
    gameplay_authorization_commitment_sha256: String,
    launch_authorization_commitment_sha256: String,
    valid_from_frame_sequence: u64,
}

impl RatifiedMtgoCompetitiveMatchLaunchV1 {
    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.authorization.event_kind
    }

    pub fn game_number_v1(&self) -> u8 {
        self.authorization.game_number
    }

    pub fn mode_authorization_commitment_sha256_v1(&self) -> &str {
        &self.mode_authorization_commitment_sha256
    }

    pub fn gameplay_authorization_commitment_sha256_v1(&self) -> &str {
        &self.gameplay_authorization_commitment_sha256
    }

    pub fn launch_authorization_commitment_sha256_v1(&self) -> &str {
        &self.launch_authorization_commitment_sha256
    }

    pub fn valid_from_frame_sequence_v2(&self) -> u64 {
        self.valid_from_frame_sequence
    }

    pub fn valid_through_frame_sequence_v1(&self) -> u64 {
        self.authorization.valid_through_frame_sequence
    }

    pub fn owner_launch_authorization_sha256_v2(&self) -> &str {
        &self.authorization.owner_launch_authorization_sha256
    }

    /// Returns the coordinate-free authorization record needed when building
    /// the exact visible match plan. The record alone grants no input authority.
    pub fn gameplay_authorization_record_v2(&self) -> MtgoCompetitiveMatchGameplayAuthorizationV1 {
        self.authorization.clone()
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveGameSessionCommitmentsV1 {
    pub session_commitment_sha256: String,
    pub general_permission_commitment_sha256: String,
    pub match_launch_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub valid_from_frame_sequence: u64,
    pub valid_through_frame_sequence: u64,
    pub last_confirmed_frame_sequence: u64,
    pub confirmed_action_count: u64,
}

/// Move-only authority for sequential visible actions in one exact already
/// entered League or Challenge game. It owns both independent ratifications.
/// The session is returned to the caller only after an emitted action has a
/// newer visible postcondition and the shared input gate is released.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveGameSessionV1;
/// let _forged = OpaqueMtgoCompetitiveGameSessionV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveGameSessionV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveGameSessionV1>();
/// ```
pub struct OpaqueMtgoCompetitiveGameSessionV1 {
    authorization: RatifiedMtgoCompetitiveDuelPassAuthorizationV1,
    match_launch: RatifiedMtgoCompetitiveMatchLaunchV1,
    session_commitment_sha256: String,
    last_confirmed_frame_sequence: u64,
    confirmed_action_count: u64,
}

impl OpaqueMtgoCompetitiveGameSessionV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveGameSessionCommitmentsV1 {
        MtgoCompetitiveGameSessionCommitmentsV1 {
            session_commitment_sha256: self.session_commitment_sha256.clone(),
            general_permission_commitment_sha256: self
                .authorization
                .authorization_commitment_sha256
                .clone(),
            match_launch_commitment_sha256: self
                .match_launch
                .launch_authorization_commitment_sha256
                .clone(),
            event_kind: self.match_launch.authorization.event_kind,
            game_number: self.match_launch.authorization.game_number,
            valid_from_frame_sequence: self.match_launch.valid_from_frame_sequence,
            valid_through_frame_sequence: self
                .match_launch
                .authorization
                .valid_through_frame_sequence,
            last_confirmed_frame_sequence: self.last_confirmed_frame_sequence,
            confirmed_action_count: self.confirmed_action_count,
        }
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

/// Coordinate-free facts shown to the account owner before authorizing
/// gameplay in one exact already-entered League or Challenge game. Event entry
/// and resource spending are deliberately outside this request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoAttendedCompetitiveMatchLaunchRequestV4 {
    pub schema_version: u32,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_display_label: String,
    pub opponent_display_name: String,
    pub visible_match_id: String,
    pub visible_game_id: String,
    pub event_identity_sha256: String,
    pub match_identity_sha256: String,
    pub game_number: u8,
    pub entry_authorization_sha256: String,
    pub observed_frame_sequence: u64,
    pub source_capture_commitment_sha256: String,
    pub source_perception_result_commitment_sha256: String,
    pub source_lifecycle_snapshot_commitment_sha256: String,
    pub source_window_title_sha256: String,
    pub source_event_label_region_sha256: String,
    pub source_launch_identity_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoAuthorizationBoundCompetitiveDuelPassCommitmentsV1 {
    pub preparation_commitment_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub before_input_postcondition_verification_commitment_sha256: String,
    pub ratified_authorization_commitment_sha256: String,
    pub ratified_match_launch_commitment_sha256: String,
    pub competitive_match_gameplay_authorization_commitment_sha256: String,
    pub competitive_game_session_commitment_sha256: String,
    pub authorization_binding_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub immediate_frame_id: u64,
    pub immediate_frame_sequence: u64,
}

/// One prepared priority Pass joined to its separately ratified exact League
/// or Challenge authority. It still cannot send input or enter an event.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1;
/// fn cannot_act(value: &OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1) {
///     let _ = value.target_point_client_px();
///     let _ = value.send_input();
///     let _ = value.enter_event();
/// }
/// ```
pub struct OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1 {
    _prepared: OpaqueMtgoPreparedCompetitiveDuelPassV1,
    _session: OpaqueMtgoCompetitiveGameSessionV1,
    commitments: MtgoAuthorizationBoundCompetitiveDuelPassCommitmentsV1,
}

impl OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1 {
    pub fn commitments_v1(&self) -> MtgoAuthorizationBoundCompetitiveDuelPassCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

/// Proof that exactly one authorization-bound competitive Pass click was
/// emitted and the shared process gate is waiting for its exact visible
/// postcondition. It exposes no pixels, HWND, or coordinates.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPendingCompetitiveDuelPassV1;
/// let _forged = OpaqueMtgoPendingCompetitiveDuelPassV1 {};
/// ```
pub struct OpaqueMtgoPendingCompetitiveDuelPassV1 {
    prepared: OpaqueMtgoPreparedCompetitiveDuelPassV1,
    session: OpaqueMtgoCompetitiveGameSessionV1,
    authorization_binding_commitments: MtgoAuthorizationBoundCompetitiveDuelPassCommitmentsV1,
    input_receipt_sha256: String,
    input_sent_at_unix_millis: u128,
    cursor_parked_outside_client: bool,
}

impl OpaqueMtgoPendingCompetitiveDuelPassV1 {
    pub fn authorization_binding_commitment_sha256_v1(&self) -> &str {
        &self
            .authorization_binding_commitments
            .authorization_binding_commitment_sha256
    }

    pub fn input_receipt_sha256_v1(&self) -> &str {
        &self.input_receipt_sha256
    }

    pub fn input_sent_at_unix_millis_v1(&self) -> u128 {
        self.input_sent_at_unix_millis
    }

    pub fn cursor_parked_outside_client_v1(&self) -> bool {
        self.cursor_parked_outside_client
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.authorization_binding_commitments.event_kind
    }

    pub fn game_number_v1(&self) -> u8 {
        self.authorization_binding_commitments.game_number
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoConfirmedCompetitiveDuelPassCommitmentsV2 {
    pub input_receipt_sha256: String,
    pub visible_postcondition_commitment_sha256: String,
    pub transition_receipt_sha256: String,
    pub prior_game_session_commitment_sha256: String,
    pub advanced_game_session_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub postcondition_candidate_count: u32,
    pub confirmed_action_count: u64,
}

/// One emitted priority Pass with its exact newer visible postcondition
/// confirmed. The shared input gate has been released. The only way to recover
/// the move-only game session for a later action is to consume this value.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoConfirmedCompetitiveDuelPassTransitionV2;
/// let _forged = OpaqueMtgoConfirmedCompetitiveDuelPassTransitionV2 {};
/// ```
pub struct OpaqueMtgoConfirmedCompetitiveDuelPassTransitionV2 {
    _confirmation: OpaqueMtgoConfirmedCompetitiveDuelPassV1,
    session: OpaqueMtgoCompetitiveGameSessionV1,
    commitments: MtgoConfirmedCompetitiveDuelPassCommitmentsV2,
}

impl OpaqueMtgoConfirmedCompetitiveDuelPassTransitionV2 {
    pub fn commitments_v2(&self) -> MtgoConfirmedCompetitiveDuelPassCommitmentsV2 {
        self.commitments.clone()
    }

    pub fn into_game_session_v1(self) -> OpaqueMtgoCompetitiveGameSessionV1 {
        self.session
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
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
    mtgo_input_gate_status_v3()
}

pub fn mtgo_input_gate_status_v3() -> Result<MtgoInputGateStatusV3, String> {
    let gate = input_gate_v3()
        .lock()
        .map_err(|_| "the process-wide input gate is poisoned".to_owned())?;
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

pub fn ratify_competitive_duel_pass_authorization_v1(
    scope: MtgoAuthorizationScopeV1,
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
) -> Result<RatifiedMtgoCompetitiveDuelPassAuthorizationV1, String> {
    ratify_competitive_duel_pass_authorization_with_commitment_v1(
        scope,
        visible_account_alias,
        event_kind,
        RATIFIED_COMPETITIVE_DUEL_PASS_AUTHORIZATION_COMMITMENT_V1,
    )
}

/// Preferred production path for general League or Challenge priority-Pass
/// permission. It consumes a structurally checked review of the exact private
/// correspondence bytes, derives a one-mode scope from that review, and then
/// requires a separately compile-pinned commitment to the review and scope.
/// The production commitment remains empty until the exact reply is imported
/// and reviewed. This function grants no event-entry or spending authority.
pub fn ratify_competitive_duel_pass_authorization_from_correspondence_v2(
    correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
) -> Result<RatifiedMtgoCompetitiveDuelPassAuthorizationV1, String> {
    ratify_competitive_duel_pass_authorization_from_correspondence_with_commitment_v2(
        correspondence,
        visible_account_alias,
        event_kind,
        RATIFIED_COMPETITIVE_DUEL_PASS_AUTHORIZATION_FROM_REVIEW_COMMITMENT_V2,
    )
}

/// Computes the exact one-mode production ratification candidate from a
/// structurally checked correspondence review without creating authority.
/// The caller must independently review the private message and the source
/// diff before placing this value in a production trust root.
pub fn review_competitive_duel_pass_ratification_candidate_from_correspondence_v2(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
) -> Result<MtgoReviewedCompetitivePassRatificationCandidateV2, String> {
    let permission_review_commitment_sha256 =
        correspondence.review_commitment_sha256().to_owned();
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(event_kind)
        .map_err(|error| format!("derive reviewed competitive mode scope: {error}"))?;
    let mode_authorization_commitment_sha256 = validate_competitive_duel_pass_authorization_v1(
        &scope,
        visible_account_alias,
        event_kind,
    )?;
    let ratification_commitment_sha256 =
        competitive_duel_pass_authorization_from_review_commitment_v2(
            &scope,
            visible_account_alias,
            event_kind,
            &mode_authorization_commitment_sha256,
            &permission_review_commitment_sha256,
        );
    Ok(MtgoReviewedCompetitivePassRatificationCandidateV2 {
        permission_review_commitment_sha256,
        account_alias_sha256: scope.account_alias_sha256,
        correspondence_sha256: scope.written_permission_sha256,
        mode_authorization_commitment_sha256,
        ratification_commitment_sha256,
        event_kind,
    })
}

pub fn ratify_competitive_match_launch_v1(
    scope: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    authorization: MtgoCompetitiveMatchGameplayAuthorizationV1,
) -> Result<RatifiedMtgoCompetitiveMatchLaunchV1, String> {
    ratify_competitive_match_launch_with_commitment_v1(
        scope,
        visible_account_alias,
        authorization,
        RATIFIED_COMPETITIVE_MATCH_LAUNCH_AUTHORIZATION_COMMITMENT_V1,
    )
}

/// Requires a source-bound visible match identity, a real interactive terminal,
/// and an exact owner-entered challenge before creating one move-only League or
/// Challenge game launch. Redirected stdin/stdout is rejected. General Daybreak
/// permission remains a separate compile-pinned prerequisite, and this function
/// grants no event-entry or resource-spending authority.
pub fn ratify_competitive_match_launch_attended_v4(
    scope: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
) -> Result<RatifiedMtgoCompetitiveMatchLaunchV1, String> {
    let source = visible_identity.commitments_v1();
    let request = MtgoAttendedCompetitiveMatchLaunchRequestV4 {
        schema_version: MTGO_ATTENDED_COMPETITIVE_MATCH_LAUNCH_REQUEST_SCHEMA_V4,
        event_kind: source.event_kind,
        event_display_label: visible_identity.event_display_label_v1().to_owned(),
        opponent_display_name: visible_identity.opponent_display_name_v1().to_owned(),
        visible_match_id: visible_identity.visible_match_id_v1().to_owned(),
        visible_game_id: visible_identity.visible_game_id_v1().to_owned(),
        event_identity_sha256: visible_identity.event_identity_sha256_v1().to_owned(),
        match_identity_sha256: visible_identity.match_identity_sha256_v1().to_owned(),
        game_number: source.game_number,
        entry_authorization_sha256: visible_identity
            .entry_authorization_sha256_v1()
            .to_owned(),
        observed_frame_sequence: source.frame_sequence,
        source_capture_commitment_sha256: source.source_capture_commitment_sha256,
        source_perception_result_commitment_sha256: source
            .perception_result_commitment_sha256,
        source_lifecycle_snapshot_commitment_sha256: source
            .lifecycle_snapshot_commitment_sha256,
        source_window_title_sha256: source.window_title_sha256,
        source_event_label_region_sha256: source.event_label_region_sha256,
        source_launch_identity_commitment_sha256: source.launch_identity_commitment_sha256,
    };
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    if !stdin.is_terminal() || !stdout.is_terminal() {
        return Err(
            "attended competitive match launch requires an interactive terminal".to_owned(),
        );
    }
    let mut challenge_nonce = [0_u8; 8];
    unsafe {
        BCryptGenRandom(None, &mut challenge_nonce, BCRYPT_USE_SYSTEM_PREFERRED_RNG)
            .ok()
            .map_err(|error| format!("generate attended launch challenge: {error}"))?;
    }
    let issued_at_unix_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before epoch: {error}"))?
        .as_millis();
    validate_attended_competitive_match_launch_request_v4(scope, visible_account_alias, &request)?;
    let expected_phrase = attended_competitive_match_launch_confirmation_phrase_v4(
        request.event_kind,
        request.game_number,
        &challenge_nonce,
    );
    let mode_label = competitive_event_kind_label_v4(request.event_kind);
    let event_prefix = &request.event_identity_sha256[..12];
    let match_prefix = &request.match_identity_sha256[..12];
    writeln!(stdout, "MTGO attended competitive match launch")
        .map_err(|error| format!("write attended launch prompt: {error}"))?;
    writeln!(stdout, "Account: {visible_account_alias}")
        .map_err(|error| format!("write attended launch account: {error}"))?;
    writeln!(
        stdout,
        "Mode: {mode_label}; event: {}; opponent: {}; game: {}; visible match id: {}; visible game id: {}; event id: {event_prefix}; match id: {match_prefix}",
        request.event_display_label,
        request.opponent_display_name,
        request.game_number,
        request.visible_match_id,
        request.visible_game_id
    )
    .map_err(|error| format!("write attended launch identity: {error}"))?;
    writeln!(
        stdout,
        "This authorizes visible priority-Pass gameplay only for this exact already-entered game. It cannot enter an event or spend resources."
    )
    .map_err(|error| format!("write attended launch scope: {error}"))?;
    writeln!(stdout, "Type exactly: {expected_phrase}")
        .map_err(|error| format!("write attended launch challenge: {error}"))?;
    stdout
        .flush()
        .map_err(|error| format!("flush attended launch prompt: {error}"))?;

    let mut supplied_phrase = String::new();
    stdin
        .read_line(&mut supplied_phrase)
        .map_err(|error| format!("read attended launch confirmation: {error}"))?;
    let supplied_phrase = supplied_phrase.trim_end_matches(['\r', '\n']);
    ratify_competitive_match_launch_from_attended_confirmation_v4(
        scope,
        visible_account_alias,
        request,
        challenge_nonce,
        issued_at_unix_millis,
        supplied_phrase,
    )
}

pub fn begin_competitive_game_session_v1(
    authorization: RatifiedMtgoCompetitiveDuelPassAuthorizationV1,
    match_launch: RatifiedMtgoCompetitiveMatchLaunchV1,
) -> Result<OpaqueMtgoCompetitiveGameSessionV1, String> {
    validate_competitive_game_session_authorities_v1(&authorization, &match_launch)?;
    let session_commitment_sha256 =
        initial_competitive_game_session_commitment_v1(&authorization, &match_launch);
    Ok(OpaqueMtgoCompetitiveGameSessionV1 {
        authorization,
        last_confirmed_frame_sequence: match_launch
            .valid_from_frame_sequence
            .checked_sub(1)
            .ok_or("competitive game session has an invalid starting frame")?,
        match_launch,
        session_commitment_sha256,
        confirmed_action_count: 0,
    })
}

pub fn bind_prepared_competitive_duel_pass_session_v2(
    prepared: OpaqueMtgoPreparedCompetitiveDuelPassV1,
    session: OpaqueMtgoCompetitiveGameSessionV1,
) -> Result<OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1, String> {
    let prepared_commitments = prepared.commitments_v1();
    let commitments = competitive_duel_pass_authorization_binding_commitments_v1(
        &prepared_commitments,
        &session,
    )?;
    Ok(OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1 {
        _prepared: prepared,
        _session: session,
        commitments,
    })
}

pub fn execute_authorized_competitive_duel_pass_v1(
    bound: OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1,
) -> Result<OpaqueMtgoPendingCompetitiveDuelPassV1, String> {
    reserve_input_gate_v3()?;
    let OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1 {
        _prepared: prepared,
        _session: session,
        commitments: authorization_binding_commitments,
    } = bound;
    let input_sent_at_unix_millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(error) => {
            release_unattempted_reservation_v3()?;
            return Err(format!("system clock is before epoch: {error}"));
        }
    };
    if validate_preinput_capture_freshness_v3(
        prepared.commitments_v1().immediate_captured_at_unix_millis,
        input_sent_at_unix_millis,
    )
    .is_err()
    {
        release_unattempted_reservation_v3()?;
        return Err(
            "the immediate competitive Pass capture is stale or from the future".to_owned(),
        );
    }
    halt_before_input_attempt_v3()?;
    let cursor_parked_outside_client = send_exactly_one_left_click_v3(&prepared)?;
    let input_receipt_sha256 = competitive_duel_pass_input_receipt_v1(
        &prepared,
        &authorization_binding_commitments,
        input_sent_at_unix_millis,
        cursor_parked_outside_client,
    );
    set_pending_v3(&input_receipt_sha256)?;
    Ok(OpaqueMtgoPendingCompetitiveDuelPassV1 {
        prepared,
        session,
        authorization_binding_commitments,
        input_receipt_sha256,
        input_sent_at_unix_millis,
        cursor_parked_outside_client,
    })
}

pub fn confirm_pending_competitive_duel_pass_v2(
    pending: OpaqueMtgoPendingCompetitiveDuelPassV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoConfirmedCompetitiveDuelPassTransitionV2, String> {
    require_matching_pending_v3(&pending.input_receipt_sha256)?;
    let OpaqueMtgoPendingCompetitiveDuelPassV1 {
        prepared,
        session,
        authorization_binding_commitments,
        input_receipt_sha256,
        input_sent_at_unix_millis: _,
        cursor_parked_outside_client: _,
    } = pending;
    let authorization_binding_commitment_sha256 =
        authorization_binding_commitments.authorization_binding_commitment_sha256;
    let confirmation = match confirm_opaque_competitive_duel_pass_postcondition_v1(
        prepared, profile, timeout_ms,
    ) {
        Ok(confirmation) => confirmation,
        Err(error) => {
            halt_gate_v3()?;
            return Err(format!(
                "competitive Pass postcondition failed and the input gate is halted: {error}"
            ));
        }
    };
    let visible = confirmation.commitments_v1();
    let transition_receipt_sha256 = competitive_duel_pass_transition_receipt_v1(
        &input_receipt_sha256,
        &authorization_binding_commitment_sha256,
        &visible,
    );
    let prior_game_session_commitment_sha256 = session.session_commitment_sha256.clone();
    let session =
        match advance_competitive_game_session_v1(session, &visible, &transition_receipt_sha256) {
            Ok(session) => session,
            Err(error) => {
                halt_gate_v3()?;
                return Err(format!(
                    "competitive game session advance failed and the input gate is halted: {error}"
                ));
            }
        };
    let advanced = session.commitments_v1();
    release_confirmed_pending_v3(&input_receipt_sha256)?;
    Ok(OpaqueMtgoConfirmedCompetitiveDuelPassTransitionV2 {
        _confirmation: confirmation,
        session,
        commitments: MtgoConfirmedCompetitiveDuelPassCommitmentsV2 {
            input_receipt_sha256,
            visible_postcondition_commitment_sha256: visible.opaque_confirmation_commitment_sha256,
            transition_receipt_sha256,
            prior_game_session_commitment_sha256,
            advanced_game_session_commitment_sha256: advanced.session_commitment_sha256,
            event_kind: visible.event_kind,
            game_number: visible.game_number,
            after_frame_id: visible.after_frame_id,
            after_frame_sequence: visible.after_frame_sequence,
            postcondition_candidate_count: visible.postcondition_candidate_count,
            confirmed_action_count: advanced.confirmed_action_count,
        },
    })
}

fn ratify_competitive_duel_pass_authorization_with_commitment_v1(
    scope: MtgoAuthorizationScopeV1,
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
    ratified_commitment_sha256: Option<&str>,
) -> Result<RatifiedMtgoCompetitiveDuelPassAuthorizationV1, String> {
    let mode_authorization_commitment_sha256 = validate_competitive_duel_pass_authorization_v1(
        &scope,
        &visible_account_alias,
        event_kind,
    )?;
    let authorization_commitment_sha256 = competitive_duel_pass_authorization_commitment_v1(
        &scope,
        &visible_account_alias,
        event_kind,
        &mode_authorization_commitment_sha256,
    );
    if ratified_commitment_sha256 != Some(authorization_commitment_sha256.as_str()) {
        return Err(
            "the exact competitive Pass permission correspondence is not ratified in this build"
                .to_owned(),
        );
    }
    Ok(RatifiedMtgoCompetitiveDuelPassAuthorizationV1 {
        scope,
        _permission_correspondence: None,
        visible_account_alias,
        event_kind,
        mode_authorization_commitment_sha256,
        authorization_commitment_sha256,
        permission_review_commitment_sha256: None,
    })
}

fn ratify_competitive_duel_pass_authorization_from_correspondence_with_commitment_v2(
    correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
    ratified_commitment_sha256: Option<&str>,
) -> Result<RatifiedMtgoCompetitiveDuelPassAuthorizationV1, String> {
    let candidate =
        review_competitive_duel_pass_ratification_candidate_from_correspondence_v2(
            &correspondence,
            &visible_account_alias,
            event_kind,
        )?;
    if ratified_commitment_sha256 != Some(candidate.ratification_commitment_sha256.as_str()) {
        return Err(
            "the exact reviewed competitive Pass permission is not ratified in this build"
                .to_owned(),
        );
    }
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(event_kind)
        .map_err(|error| format!("derive reviewed competitive mode scope: {error}"))?;
    Ok(RatifiedMtgoCompetitiveDuelPassAuthorizationV1 {
        scope,
        _permission_correspondence: Some(correspondence),
        visible_account_alias,
        event_kind,
        mode_authorization_commitment_sha256: candidate.mode_authorization_commitment_sha256,
        authorization_commitment_sha256: candidate.ratification_commitment_sha256,
        permission_review_commitment_sha256: Some(candidate.permission_review_commitment_sha256),
    })
}

fn ratify_competitive_match_launch_from_attended_confirmation_v4(
    scope: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    request: MtgoAttendedCompetitiveMatchLaunchRequestV4,
    challenge_nonce: [u8; 8],
    issued_at_unix_millis: u128,
    supplied_phrase: &str,
) -> Result<RatifiedMtgoCompetitiveMatchLaunchV1, String> {
    let mode_authorization_commitment_sha256 =
        validate_attended_competitive_match_launch_request_v4(
            scope,
            visible_account_alias,
            &request,
        )?;
    let expected_phrase = attended_competitive_match_launch_confirmation_phrase_v4(
        request.event_kind,
        request.game_number,
        &challenge_nonce,
    );
    if supplied_phrase != expected_phrase {
        return Err("attended competitive match launch challenge did not match".to_owned());
    }
    let request_json = serde_json::to_vec(&request)
        .map_err(|error| format!("serialize attended match launch request: {error}"))?;
    let request_commitment_sha256 = hash_parts_v2(
        ATTENDED_COMPETITIVE_MATCH_LAUNCH_REQUEST_DOMAIN_V4,
        &[
            scope.account_alias_sha256.as_bytes(),
            scope.written_permission_sha256.as_bytes(),
            visible_account_alias.as_bytes(),
            mode_authorization_commitment_sha256.as_bytes(),
            request_json.as_slice(),
        ],
    );
    let owner_launch_authorization_sha256 = hash_parts_v2(
        ATTENDED_COMPETITIVE_MATCH_LAUNCH_RECEIPT_DOMAIN_V4,
        &[
            request_commitment_sha256.as_bytes(),
            challenge_nonce.as_slice(),
            issued_at_unix_millis.to_be_bytes().as_slice(),
            supplied_phrase.as_bytes(),
            b"interactive_terminal_owner_confirmation_priority_pass_only",
        ],
    );
    let valid_through_frame_sequence = request
        .observed_frame_sequence
        .checked_add(ATTENDED_COMPETITIVE_MATCH_MAX_FRAME_ADVANCE_V4)
        .ok_or("attended competitive match frame lifetime overflow")?;
    let authorization = MtgoCompetitiveMatchGameplayAuthorizationV1 {
        schema_version: MTGO_COMPETITIVE_MATCH_GAMEPLAY_AUTHORIZATION_SCHEMA_V1,
        account_alias_sha256: scope.account_alias_sha256.clone(),
        written_permission_sha256: scope.written_permission_sha256.clone(),
        event_kind: request.event_kind,
        event_identity_sha256: request.event_identity_sha256,
        match_identity_sha256: request.match_identity_sha256,
        game_number: request.game_number,
        entry_authorization_sha256: request.entry_authorization_sha256,
        owner_launch_authorization_sha256,
        exact_match_gameplay_authorized: true,
        valid_through_frame_sequence,
    };
    let gameplay_authorization_commitment_sha256 =
        competitive_match_gameplay_authorization_commitment_v1(&authorization)
            .map_err(|error| format!("competitive match authorization commitment: {error}"))?;
    let launch_authorization_commitment_sha256 = competitive_match_launch_commitment_v1(
        visible_account_alias,
        &mode_authorization_commitment_sha256,
        &gameplay_authorization_commitment_sha256,
        &authorization,
    );
    let mut ratified = ratify_competitive_match_launch_with_commitment_v1(
        scope,
        visible_account_alias,
        authorization,
        Some(&launch_authorization_commitment_sha256),
    )?;
    ratified.valid_from_frame_sequence = request.observed_frame_sequence;
    Ok(ratified)
}

fn validate_attended_competitive_match_launch_request_v4(
    scope: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    request: &MtgoAttendedCompetitiveMatchLaunchRequestV4,
) -> Result<String, String> {
    if request.schema_version != MTGO_ATTENDED_COMPETITIVE_MATCH_LAUNCH_REQUEST_SCHEMA_V4
        || !(1..=3).contains(&request.game_number)
        || request.observed_frame_sequence == 0
    {
        return Err("attended competitive match launch request is invalid".to_owned());
    }
    validate_attended_launch_display_label_v4(visible_account_alias, 64, "account alias")?;
    validate_attended_launch_display_label_v4(
        &request.event_display_label,
        160,
        "event display label",
    )?;
    validate_attended_launch_display_label_v4(
        &request.opponent_display_name,
        64,
        "opponent display name",
    )?;
    if request
        .opponent_display_name
        .eq_ignore_ascii_case(visible_account_alias)
    {
        return Err("attended opponent display name must differ from the account alias".to_owned());
    }
    for (value, field) in [
        (request.visible_match_id.as_str(), "visible match ID"),
        (request.visible_game_id.as_str(), "visible game ID"),
    ] {
        if value.is_empty()
            || value.len() > 32
            || !value.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(format!("attended {field} must be a bounded decimal integer"));
        }
    }
    let mode_authorization_commitment_sha256 = validate_competitive_duel_pass_authorization_v1(
        scope,
        visible_account_alias,
        request.event_kind,
    )?;
    for value in [
        request.event_identity_sha256.as_str(),
        request.match_identity_sha256.as_str(),
        request.entry_authorization_sha256.as_str(),
        request.source_capture_commitment_sha256.as_str(),
        request.source_perception_result_commitment_sha256.as_str(),
        request.source_lifecycle_snapshot_commitment_sha256.as_str(),
        request.source_window_title_sha256.as_str(),
        request.source_event_label_region_sha256.as_str(),
        request.source_launch_identity_commitment_sha256.as_str(),
    ] {
        if !is_sha256_v2(value) {
            return Err(
                "attended competitive match launch contains an invalid commitment".to_owned(),
            );
        }
    }
    if request.event_identity_sha256 == request.match_identity_sha256
        || request.event_identity_sha256 == request.entry_authorization_sha256
        || request.match_identity_sha256 == request.entry_authorization_sha256
        || request.entry_authorization_sha256 == scope.written_permission_sha256
    {
        return Err(
            "attended event, match, entry, and permission records must be distinct".to_owned(),
        );
    }
    request
        .observed_frame_sequence
        .checked_add(ATTENDED_COMPETITIVE_MATCH_MAX_FRAME_ADVANCE_V4)
        .ok_or("attended competitive match frame lifetime overflow")?;
    Ok(mode_authorization_commitment_sha256)
}

fn attended_competitive_match_launch_confirmation_phrase_v4(
    event_kind: MtgoCompetitiveEventKindV1,
    game_number: u8,
    challenge_nonce: &[u8; 8],
) -> String {
    let nonce = challenge_nonce
        .iter()
        .map(|value| format!("{value:02X}"))
        .collect::<String>();
    format!(
        "AUTHORIZE MTGO {} GAME {game_number} {nonce}",
        competitive_event_kind_label_v4(event_kind).to_ascii_uppercase()
    )
}

fn competitive_event_kind_label_v4(event_kind: MtgoCompetitiveEventKindV1) -> &'static str {
    match event_kind {
        MtgoCompetitiveEventKindV1::League => "League",
        MtgoCompetitiveEventKindV1::Challenge => "Challenge",
    }
}

fn validate_attended_launch_display_label_v4(
    value: &str,
    max_bytes: usize,
    field: &str,
) -> Result<(), String> {
    if value.is_empty()
        || value.len() > max_bytes
        || value.trim() != value
        || !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
    {
        return Err(format!(
            "attended {field} must be nonempty, trimmed, bounded ASCII display text"
        ));
    }
    Ok(())
}

fn hash_parts_v2(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        update_hash_part_v3(&mut hasher, part);
    }
    format!("{:x}", hasher.finalize())
}

fn is_sha256_v2(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn ratify_competitive_match_launch_with_commitment_v1(
    scope: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    authorization: MtgoCompetitiveMatchGameplayAuthorizationV1,
    ratified_commitment_sha256: Option<&str>,
) -> Result<RatifiedMtgoCompetitiveMatchLaunchV1, String> {
    let mode_authorization_commitment_sha256 = validate_competitive_duel_pass_authorization_v1(
        scope,
        visible_account_alias,
        authorization.event_kind,
    )?;
    validate_competitive_match_launch_record_v1(scope, &authorization)?;
    let gameplay_authorization_commitment_sha256 =
        competitive_match_gameplay_authorization_commitment_v1(&authorization)
            .map_err(|error| format!("competitive match authorization commitment: {error}"))?;
    let launch_authorization_commitment_sha256 = competitive_match_launch_commitment_v1(
        visible_account_alias,
        &mode_authorization_commitment_sha256,
        &gameplay_authorization_commitment_sha256,
        &authorization,
    );
    if ratified_commitment_sha256 != Some(launch_authorization_commitment_sha256.as_str()) {
        return Err(
            "the exact competitive match owner launch is not ratified in this build".to_owned(),
        );
    }
    Ok(RatifiedMtgoCompetitiveMatchLaunchV1 {
        authorization,
        mode_authorization_commitment_sha256,
        gameplay_authorization_commitment_sha256,
        launch_authorization_commitment_sha256,
        valid_from_frame_sequence: 1,
    })
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

pub fn confirm_pending_pregame_keep_to_bottom_six_v3(
    pending: OpaqueMtgoPendingPregameInputV3,
    after: OpaqueMtgoDxgiBottomSixInitialMeasurementV3,
) -> Result<OpaqueMtgoConfirmedKeepToBottomSixTransitionV3, String> {
    require_matching_pending_v3(&pending.input_receipt_sha256)?;
    match confirm_pregame_keep_to_bottom_six_transition_v3(pending.plan, after) {
        Ok(confirmed) => {
            release_confirmed_pending_v3(&pending.input_receipt_sha256)?;
            Ok(confirmed)
        }
        Err(error) => {
            halt_gate_v3()?;
            Err(format!(
                "pregame Keep-to-bottom-six postcondition failed and the input gate is halted: {error}"
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
    validate_visible_account_alias_v1(authorization, visible_account_alias)?;
    Ok(())
}

fn validate_visible_account_alias_v1(
    authorization: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
) -> Result<(), String> {
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

fn validate_competitive_duel_pass_authorization_v1(
    authorization: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
) -> Result<String, String> {
    let runtime_mode = match event_kind {
        MtgoCompetitiveEventKindV1::League => MtgoRuntimeModeV1::LeagueInput,
        MtgoCompetitiveEventKindV1::Challenge => MtgoRuntimeModeV1::ChallengeInput,
    };
    validate_authorization_for_mode_v1(authorization, runtime_mode)
        .map_err(|error| format!("competitive Pass authorization rejected: {error}"))?;
    validate_visible_account_alias_v1(authorization, visible_account_alias)?;

    let selected_mode_is_exact = match event_kind {
        MtgoCompetitiveEventKindV1::League => {
            authorization.league_input && !authorization.challenge_input
        }
        MtgoCompetitiveEventKindV1::Challenge => {
            authorization.challenge_input && !authorization.league_input
        }
    };
    if !selected_mode_is_exact
        || authorization.shadow_observation
        || authorization.private_match_input
        || authorization.open_play_input
        || authorization.other_prize_event_input
    {
        return Err(
            "competitive Pass ratification requires exactly one League or Challenge input mode"
                .to_owned(),
        );
    }
    competitive_mode_authorization_commitment_v1(authorization, event_kind)
        .map_err(|error| format!("competitive Pass mode commitment rejected: {error}"))
}

fn competitive_duel_pass_authorization_commitment_v1(
    authorization: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
    mode_authorization_commitment_sha256: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(COMPETITIVE_DUEL_PASS_AUTHORIZATION_DOMAIN_V1);
    let schema_version = authorization.schema_version.to_be_bytes();
    let event_kind_bytes: &[u8] = match event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    for part in [
        schema_version.as_slice(),
        authorization.account_alias_sha256.as_bytes(),
        authorization.written_permission_sha256.as_bytes(),
        visible_account_alias.as_bytes(),
        event_kind_bytes,
        mode_authorization_commitment_sha256.as_bytes(),
        b"priority_pass",
        b"one_verified_left_click",
        b"no_event_entry_or_purchase_authority",
    ] {
        update_hash_part_v3(&mut hasher, part);
    }
    format!("{:x}", hasher.finalize())
}

fn competitive_duel_pass_authorization_from_review_commitment_v2(
    authorization: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
    mode_authorization_commitment_sha256: &str,
    permission_review_commitment_sha256: &str,
) -> String {
    let base_commitment_sha256 = competitive_duel_pass_authorization_commitment_v1(
        authorization,
        visible_account_alias,
        event_kind,
        mode_authorization_commitment_sha256,
    );
    hash_parts_v2(
        COMPETITIVE_DUEL_PASS_AUTHORIZATION_FROM_REVIEW_DOMAIN_V2,
        &[
            base_commitment_sha256.as_bytes(),
            permission_review_commitment_sha256.as_bytes(),
            b"exact_correspondence_bytes_and_human_review_required",
        ],
    )
}

fn validate_competitive_match_launch_record_v1(
    scope: &MtgoAuthorizationScopeV1,
    authorization: &MtgoCompetitiveMatchGameplayAuthorizationV1,
) -> Result<(), String> {
    if authorization.schema_version != MTGO_COMPETITIVE_MATCH_GAMEPLAY_AUTHORIZATION_SCHEMA_V1
        || !authorization.exact_match_gameplay_authorized
        || !(1..=3).contains(&authorization.game_number)
        || authorization.valid_through_frame_sequence == 0
        || authorization.account_alias_sha256 != scope.account_alias_sha256
        || authorization.written_permission_sha256 != scope.written_permission_sha256
    {
        return Err("the competitive match launch record is inactive or mismatched".to_owned());
    }
    for value in [
        authorization.account_alias_sha256.as_str(),
        authorization.written_permission_sha256.as_str(),
        authorization.event_identity_sha256.as_str(),
        authorization.match_identity_sha256.as_str(),
        authorization.entry_authorization_sha256.as_str(),
        authorization.owner_launch_authorization_sha256.as_str(),
    ] {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("the competitive match launch contains an invalid commitment".to_owned());
        }
    }
    if authorization.event_identity_sha256 == authorization.match_identity_sha256
        || authorization.written_permission_sha256 == authorization.entry_authorization_sha256
        || authorization.written_permission_sha256
            == authorization.owner_launch_authorization_sha256
        || authorization.entry_authorization_sha256
            == authorization.owner_launch_authorization_sha256
    {
        return Err(
            "competitive permission, entry, match, and launch records must be distinct".to_owned(),
        );
    }
    Ok(())
}

fn competitive_match_launch_commitment_v1(
    visible_account_alias: &str,
    mode_authorization_commitment_sha256: &str,
    gameplay_authorization_commitment_sha256: &str,
    authorization: &MtgoCompetitiveMatchGameplayAuthorizationV1,
) -> String {
    let event_kind_bytes: &[u8] = match authorization.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let mut hasher = Sha256::new();
    hasher.update(COMPETITIVE_MATCH_LAUNCH_AUTHORIZATION_DOMAIN_V1);
    for part in [
        visible_account_alias.as_bytes(),
        mode_authorization_commitment_sha256.as_bytes(),
        gameplay_authorization_commitment_sha256.as_bytes(),
        event_kind_bytes,
        authorization.event_identity_sha256.as_bytes(),
        authorization.match_identity_sha256.as_bytes(),
        &[authorization.game_number],
        authorization.entry_authorization_sha256.as_bytes(),
        authorization.owner_launch_authorization_sha256.as_bytes(),
        authorization
            .valid_through_frame_sequence
            .to_be_bytes()
            .as_slice(),
        b"owner_launched_exact_match_priority_pass_only_no_event_entry_authority",
    ] {
        update_hash_part_v3(&mut hasher, part);
    }
    format!("{:x}", hasher.finalize())
}

fn validate_competitive_game_session_authorities_v1(
    authorization: &RatifiedMtgoCompetitiveDuelPassAuthorizationV1,
    match_launch: &RatifiedMtgoCompetitiveMatchLaunchV1,
) -> Result<(), String> {
    if authorization.event_kind != match_launch.authorization.event_kind
        || authorization.mode_authorization_commitment_sha256
            != match_launch.mode_authorization_commitment_sha256
        || authorization.scope.account_alias_sha256
            != match_launch.authorization.account_alias_sha256
        || authorization.scope.written_permission_sha256
            != match_launch.authorization.written_permission_sha256
        || match_launch.valid_from_frame_sequence == 0
        || match_launch.valid_from_frame_sequence
            > match_launch.authorization.valid_through_frame_sequence
    {
        return Err(
            "competitive game session authorities describe different modes or lifetimes".to_owned(),
        );
    }
    validate_competitive_match_launch_record_v1(&authorization.scope, &match_launch.authorization)?;
    let expected_mode = validate_competitive_duel_pass_authorization_v1(
        &authorization.scope,
        &authorization.visible_account_alias,
        authorization.event_kind,
    )?;
    if expected_mode != authorization.mode_authorization_commitment_sha256 {
        return Err("competitive game session mode commitment changed".to_owned());
    }
    let expected_gameplay =
        competitive_match_gameplay_authorization_commitment_v1(&match_launch.authorization)
            .map_err(|error| format!("competitive game session match commitment: {error}"))?;
    let expected_launch = competitive_match_launch_commitment_v1(
        &authorization.visible_account_alias,
        &expected_mode,
        &expected_gameplay,
        &match_launch.authorization,
    );
    if expected_gameplay != match_launch.gameplay_authorization_commitment_sha256
        || expected_launch != match_launch.launch_authorization_commitment_sha256
    {
        return Err("competitive game session match launch commitment changed".to_owned());
    }
    Ok(())
}

fn initial_competitive_game_session_commitment_v1(
    authorization: &RatifiedMtgoCompetitiveDuelPassAuthorizationV1,
    match_launch: &RatifiedMtgoCompetitiveMatchLaunchV1,
) -> String {
    let event_kind_bytes: &[u8] = match authorization.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    hash_parts_v2(
        COMPETITIVE_GAME_SESSION_INITIAL_DOMAIN_V1,
        &[
            authorization.authorization_commitment_sha256.as_bytes(),
            match_launch
                .launch_authorization_commitment_sha256
                .as_bytes(),
            match_launch
                .gameplay_authorization_commitment_sha256
                .as_bytes(),
            authorization
                .mode_authorization_commitment_sha256
                .as_bytes(),
            authorization.scope.account_alias_sha256.as_bytes(),
            authorization.scope.written_permission_sha256.as_bytes(),
            event_kind_bytes,
            &[match_launch.authorization.game_number],
            match_launch
                .valid_from_frame_sequence
                .to_be_bytes()
                .as_slice(),
            match_launch
                .authorization
                .valid_through_frame_sequence
                .to_be_bytes()
                .as_slice(),
            b"move_only_sequential_visible_postconditions_no_entry_or_spending",
        ],
    )
}

fn advance_competitive_game_session_v1(
    mut session: OpaqueMtgoCompetitiveGameSessionV1,
    visible: &MtgoOpaqueCompetitiveDuelPassConfirmationCommitmentsV1,
    transition_receipt_sha256: &str,
) -> Result<OpaqueMtgoCompetitiveGameSessionV1, String> {
    if visible.event_kind != session.match_launch.authorization.event_kind
        || visible.game_number != session.match_launch.authorization.game_number
        || visible.after_frame_sequence <= session.last_confirmed_frame_sequence
        || visible.after_frame_sequence
            > session
                .match_launch
                .authorization
                .valid_through_frame_sequence
        || !is_sha256_v2(transition_receipt_sha256)
    {
        return Err(
            "confirmed transition is outside the competitive game session lifetime".to_owned(),
        );
    }
    let next_count = session
        .confirmed_action_count
        .checked_add(1)
        .ok_or("competitive game session action count overflow")?;
    session.session_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_GAME_SESSION_ADVANCE_DOMAIN_V1,
        &[
            session.session_commitment_sha256.as_bytes(),
            transition_receipt_sha256.as_bytes(),
            visible.opaque_confirmation_commitment_sha256.as_bytes(),
            visible.after_frame_id.to_be_bytes().as_slice(),
            visible.after_frame_sequence.to_be_bytes().as_slice(),
            next_count.to_be_bytes().as_slice(),
            b"returned_only_after_newer_visible_postcondition",
        ],
    );
    session.last_confirmed_frame_sequence = visible.after_frame_sequence;
    session.confirmed_action_count = next_count;
    Ok(session)
}

fn competitive_duel_pass_authorization_binding_commitments_v1(
    prepared: &MtgoOpaqueCompetitiveDuelPassPreparationCommitmentsV1,
    session: &OpaqueMtgoCompetitiveGameSessionV1,
) -> Result<MtgoAuthorizationBoundCompetitiveDuelPassCommitmentsV1, String> {
    let authorization = &session.authorization;
    let match_launch = &session.match_launch;
    if prepared.event_kind != authorization.event_kind
        || prepared.competitive_mode_authorization_commitment_sha256
            != authorization.mode_authorization_commitment_sha256
        || prepared.event_kind != match_launch.authorization.event_kind
        || prepared.game_number != match_launch.authorization.game_number
        || prepared.competitive_mode_authorization_commitment_sha256
            != match_launch.mode_authorization_commitment_sha256
        || prepared.competitive_match_gameplay_authorization_commitment_sha256
            != match_launch.gameplay_authorization_commitment_sha256
        || prepared.immediate_frame_sequence < match_launch.valid_from_frame_sequence
        || prepared.immediate_frame_sequence
            > match_launch.authorization.valid_through_frame_sequence
        || prepared.immediate_frame_sequence <= session.last_confirmed_frame_sequence
    {
        return Err(
            "the prepared Pass does not match the ratified competitive mode authority".to_owned(),
        );
    }
    let event_kind_bytes: &[u8] = match prepared.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let mut hasher = Sha256::new();
    hasher.update(COMPETITIVE_DUEL_PASS_AUTHORIZATION_BINDING_DOMAIN_V1);
    for part in [
        prepared.preparation_commitment_sha256.as_bytes(),
        prepared
            .competitive_mode_authorization_commitment_sha256
            .as_bytes(),
        prepared
            .before_input_postcondition_verification_commitment_sha256
            .as_bytes(),
        authorization.authorization_commitment_sha256.as_bytes(),
        match_launch
            .launch_authorization_commitment_sha256
            .as_bytes(),
        session.session_commitment_sha256.as_bytes(),
        session
            .last_confirmed_frame_sequence
            .to_be_bytes()
            .as_slice(),
        session.confirmed_action_count.to_be_bytes().as_slice(),
        prepared
            .competitive_match_gameplay_authorization_commitment_sha256
            .as_bytes(),
        match_launch
            .valid_from_frame_sequence
            .to_be_bytes()
            .as_slice(),
        match_launch
            .authorization
            .valid_through_frame_sequence
            .to_be_bytes()
            .as_slice(),
        event_kind_bytes,
        &[prepared.game_number],
        prepared.immediate_frame_id.to_be_bytes().as_slice(),
        prepared.immediate_frame_sequence.to_be_bytes().as_slice(),
        b"authorization_bound_no_input_or_event_entry_authority",
    ] {
        update_hash_part_v3(&mut hasher, part);
    }
    Ok(MtgoAuthorizationBoundCompetitiveDuelPassCommitmentsV1 {
        preparation_commitment_sha256: prepared.preparation_commitment_sha256.clone(),
        mode_authorization_commitment_sha256: prepared
            .competitive_mode_authorization_commitment_sha256
            .clone(),
        before_input_postcondition_verification_commitment_sha256: prepared
            .before_input_postcondition_verification_commitment_sha256
            .clone(),
        ratified_authorization_commitment_sha256: authorization
            .authorization_commitment_sha256
            .clone(),
        ratified_match_launch_commitment_sha256: match_launch
            .launch_authorization_commitment_sha256
            .clone(),
        competitive_match_gameplay_authorization_commitment_sha256: prepared
            .competitive_match_gameplay_authorization_commitment_sha256
            .clone(),
        competitive_game_session_commitment_sha256: session.session_commitment_sha256.clone(),
        authorization_binding_commitment_sha256: format!("{:x}", hasher.finalize()),
        event_kind: prepared.event_kind,
        game_number: prepared.game_number,
        immediate_frame_id: prepared.immediate_frame_id,
        immediate_frame_sequence: prepared.immediate_frame_sequence,
    })
}

fn competitive_duel_pass_input_receipt_v1(
    prepared: &OpaqueMtgoPreparedCompetitiveDuelPassV1,
    authorization_binding: &MtgoAuthorizationBoundCompetitiveDuelPassCommitmentsV1,
    input_sent_at_unix_millis: u128,
    cursor_parked_outside_client: bool,
) -> String {
    let prepared_commitments = prepared.commitments_v1();
    let mut hasher = Sha256::new();
    hasher.update(COMPETITIVE_DUEL_PASS_INPUT_RECEIPT_DOMAIN_V1);
    for part in [
        authorization_binding
            .authorization_binding_commitment_sha256
            .as_bytes(),
        prepared_commitments
            .preparation_commitment_sha256
            .as_bytes(),
        prepared_commitments
            .immediate_capture_commitment_sha256
            .as_bytes(),
        prepared_commitments
            .before_input_postcondition_verification_commitment_sha256
            .as_bytes(),
        prepared.target_x_desktop_px.to_be_bytes().as_slice(),
        prepared.target_y_desktop_px.to_be_bytes().as_slice(),
        prepared.park_x_desktop_px.to_be_bytes().as_slice(),
        prepared.park_y_desktop_px.to_be_bytes().as_slice(),
        input_sent_at_unix_millis.to_be_bytes().as_slice(),
        &[u8::from(cursor_parked_outside_client)],
        b"exactly_one_priority_pass_left_click",
    ] {
        update_hash_part_v3(&mut hasher, part);
    }
    format!("{:x}", hasher.finalize())
}

fn competitive_duel_pass_transition_receipt_v1(
    input_receipt_sha256: &str,
    authorization_binding_commitment_sha256: &str,
    visible: &MtgoOpaqueCompetitiveDuelPassConfirmationCommitmentsV1,
) -> String {
    let event_kind_bytes: &[u8] = match visible.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let mut hasher = Sha256::new();
    hasher.update(COMPETITIVE_DUEL_PASS_TRANSITION_RECEIPT_DOMAIN_V1);
    for part in [
        input_receipt_sha256.as_bytes(),
        authorization_binding_commitment_sha256.as_bytes(),
        visible
            .before_input_verification_commitment_sha256
            .as_bytes(),
        visible.after_capture_commitment_sha256.as_bytes(),
        visible.checked_postcondition_commitment_sha256.as_bytes(),
        visible.opaque_confirmation_commitment_sha256.as_bytes(),
        event_kind_bytes,
        &[visible.game_number],
        visible.after_frame_id.to_be_bytes().as_slice(),
        visible.after_frame_sequence.to_be_bytes().as_slice(),
        visible
            .postcondition_candidate_count
            .to_be_bytes()
            .as_slice(),
        b"visible_postcondition_confirmed_shared_input_gate_released",
    ] {
        update_hash_part_v3(&mut hasher, part);
    }
    format!("{:x}", hasher.finalize())
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
        .map_err(|_| "the process-wide input gate is poisoned".to_owned())?;
    match &*gate {
        PregameInputGateStateV3::Idle => {
            *gate = PregameInputGateStateV3::Preparing;
            Ok(())
        }
        PregameInputGateStateV3::Preparing => {
            Err("another MTGO input is already being prepared".to_owned())
        }
        PregameInputGateStateV3::AwaitingVisiblePostcondition { .. } => {
            Err("a visible postcondition is still pending".to_owned())
        }
        PregameInputGateStateV3::Halted => Err("the process-wide input gate is halted".to_owned()),
    }
}

fn release_unattempted_reservation_v3() -> Result<(), String> {
    let mut gate = input_gate_v3()
        .lock()
        .map_err(|_| "the process-wide input gate is poisoned".to_owned())?;
    if !matches!(*gate, PregameInputGateStateV3::Preparing) {
        *gate = PregameInputGateStateV3::Halted;
        return Err("the input reservation changed unexpectedly".to_owned());
    }
    *gate = PregameInputGateStateV3::Idle;
    Ok(())
}

fn halt_before_input_attempt_v3() -> Result<(), String> {
    let mut gate = input_gate_v3()
        .lock()
        .map_err(|_| "the process-wide input gate is poisoned".to_owned())?;
    if !matches!(*gate, PregameInputGateStateV3::Preparing) {
        *gate = PregameInputGateStateV3::Halted;
        return Err("the input reservation changed unexpectedly".to_owned());
    }
    *gate = PregameInputGateStateV3::Halted;
    Ok(())
}

fn set_pending_v3(receipt_sha256: &str) -> Result<(), String> {
    let mut gate = input_gate_v3()
        .lock()
        .map_err(|_| "the process-wide input gate is poisoned".to_owned())?;
    if !matches!(*gate, PregameInputGateStateV3::Halted) {
        *gate = PregameInputGateStateV3::Halted;
        return Err("the process-wide input gate was not armed for an input attempt".to_owned());
    }
    *gate = PregameInputGateStateV3::AwaitingVisiblePostcondition {
        receipt_sha256: receipt_sha256.to_owned(),
    };
    Ok(())
}

fn require_matching_pending_v3(receipt_sha256: &str) -> Result<(), String> {
    let gate = input_gate_v3()
        .lock()
        .map_err(|_| "the process-wide input gate is poisoned".to_owned())?;
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
        .map_err(|_| "the process-wide input gate is poisoned".to_owned())?;
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
        .map_err(|_| "the process-wide input gate is poisoned".to_owned())?;
    *gate = PregameInputGateStateV3::Halted;
    Ok(())
}

trait VerifiedPointerTargetV3 {
    fn hwnd_v3(&self) -> u64;
    fn process_id_v3(&self) -> u32;
    fn process_start_filetime_100ns_v3(&self) -> u64;
    fn dpi_v3(&self) -> u32;
    fn client_rect_desktop_px_v3(&self) -> &crate::SignedRectV1;
    fn target_x_desktop_px_v3(&self) -> i32;
    fn target_y_desktop_px_v3(&self) -> i32;
    fn park_x_desktop_px_v3(&self) -> i32;
    fn park_y_desktop_px_v3(&self) -> i32;
}

impl VerifiedPointerTargetV3 for PreparedPregameActuationV3 {
    fn hwnd_v3(&self) -> u64 {
        self.hwnd
    }

    fn process_id_v3(&self) -> u32 {
        self.process_id
    }

    fn process_start_filetime_100ns_v3(&self) -> u64 {
        self.process_start_filetime_100ns
    }

    fn dpi_v3(&self) -> u32 {
        self.dpi
    }

    fn client_rect_desktop_px_v3(&self) -> &crate::SignedRectV1 {
        &self.client_rect_desktop_px
    }

    fn target_x_desktop_px_v3(&self) -> i32 {
        self.target_x_desktop_px
    }

    fn target_y_desktop_px_v3(&self) -> i32 {
        self.target_y_desktop_px
    }

    fn park_x_desktop_px_v3(&self) -> i32 {
        self.park_x_desktop_px
    }

    fn park_y_desktop_px_v3(&self) -> i32 {
        self.park_y_desktop_px
    }
}

impl VerifiedPointerTargetV3 for OpaqueMtgoPreparedCompetitiveDuelPassV1 {
    fn hwnd_v3(&self) -> u64 {
        self.hwnd
    }

    fn process_id_v3(&self) -> u32 {
        self.process_id
    }

    fn process_start_filetime_100ns_v3(&self) -> u64 {
        self.process_start_filetime_100ns
    }

    fn dpi_v3(&self) -> u32 {
        self.dpi
    }

    fn client_rect_desktop_px_v3(&self) -> &crate::SignedRectV1 {
        &self.client_rect_desktop_px
    }

    fn target_x_desktop_px_v3(&self) -> i32 {
        self.target_x_desktop_px
    }

    fn target_y_desktop_px_v3(&self) -> i32 {
        self.target_y_desktop_px
    }

    fn park_x_desktop_px_v3(&self) -> i32 {
        self.park_x_desktop_px
    }

    fn park_y_desktop_px_v3(&self) -> i32 {
        self.park_y_desktop_px
    }
}

fn send_exactly_one_left_click_v3<T: VerifiedPointerTargetV3>(
    prepared: &T,
) -> Result<bool, String> {
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
        x: prepared.park_x_desktop_px_v3(),
        y: prepared.park_y_desktop_px_v3(),
        parked: false,
    };
    unsafe {
        SetCursorPos(
            prepared.target_x_desktop_px_v3(),
            prepared.target_y_desktop_px_v3(),
        )
    }
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

fn verify_live_target_v3<T: VerifiedPointerTargetV3>(
    prepared: &T,
    require_point_hit_test: bool,
) -> Result<(), String> {
    let hwnd = HWND(prepared.hwnd_v3() as *mut c_void);
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
    if process_id != prepared.process_id_v3()
        || unsafe { GetDpiForWindow(hwnd) } != prepared.dpi_v3()
    {
        return Err("the authorized MTGO process or DPI changed before input".to_owned());
    }
    if live_process_start_filetime_v3(prepared.process_id_v3())?
        != prepared.process_start_filetime_100ns_v3()
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
    if prepared.client_rect_desktop_px_v3().left != origin.x
        || prepared.client_rect_desktop_px_v3().top != origin.y
        || prepared.client_rect_desktop_px_v3().right != right
        || prepared.client_rect_desktop_px_v3().bottom != bottom
    {
        return Err("the MTGO client geometry changed before input".to_owned());
    }

    if require_point_hit_test {
        let hit = unsafe {
            WindowFromPoint(POINT {
                x: prepared.target_x_desktop_px_v3(),
                y: prepared.target_y_desktop_px_v3(),
            })
        };
        if hit.0.is_null() || unsafe { GetAncestor(hit, GA_ROOT) } != hwnd {
            return Err("another window owns the selected control point".to_owned());
        }
        let mut hit_process_id = 0_u32;
        unsafe { GetWindowThreadProcessId(hit, Some(&mut hit_process_id)) };
        if hit_process_id != prepared.process_id_v3() {
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
    use mtgo_blackbox_v1::{
        check_untrusted_authorization_correspondence_v1, MtgoAuthorizationCorrespondenceReviewV1,
        MTGO_AUTHORIZATION_CORRESPONDENCE_REVIEW_SCHEMA_V1,
    };

    fn authorized_scope_v3(alias: &str) -> MtgoAuthorizationScopeV1 {
        MtgoAuthorizationScopeV1 {
            account_alias_sha256: format!("{:x}", Sha256::digest(alias.as_bytes())),
            written_permission_sha256: "a".repeat(64),
            private_match_input: true,
            ..MtgoAuthorizationScopeV1::default()
        }
    }

    fn competitive_scope_v1(
        alias: &str,
        event_kind: MtgoCompetitiveEventKindV1,
    ) -> MtgoAuthorizationScopeV1 {
        let mut scope = MtgoAuthorizationScopeV1 {
            account_alias_sha256: format!("{:x}", Sha256::digest(alias.as_bytes())),
            written_permission_sha256: "b".repeat(64),
            ..MtgoAuthorizationScopeV1::default()
        };
        match event_kind {
            MtgoCompetitiveEventKindV1::League => scope.league_input = true,
            MtgoCompetitiveEventKindV1::Challenge => scope.challenge_input = true,
        }
        scope
    }

    fn checked_competitive_correspondence_v2() -> CheckedUntrustedMtgoAuthorizationCorrespondenceV1
    {
        const BYTES: &[u8] = b"exact private Daybreak correspondence test fixture\r\n";
        let account_alias = "UnbuckledPie";
        check_untrusted_authorization_correspondence_v1(
            MtgoAuthorizationCorrespondenceReviewV1 {
                schema_version: MTGO_AUTHORIZATION_CORRESPONDENCE_REVIEW_SCHEMA_V1,
                review_id: "daybreak-visible-competitive-test-v1".to_owned(),
                correspondence_sha256: format!("{:x}", Sha256::digest(BYTES)),
                approved_account_alias_sha256: format!(
                    "{:x}",
                    Sha256::digest(account_alias.as_bytes())
                ),
                approved_competitive_modes: vec![
                    MtgoCompetitiveEventKindV1::League,
                    MtgoCompetitiveEventKindV1::Challenge,
                ],
                visible_channels_only: true,
                hidden_information_access_prohibited: true,
                hidden_information_reverse_engineering_prohibited: true,
                cheating_or_hacking_prohibited: true,
                account_owner_event_entry_confirmation_required: true,
                account_owner_spending_confirmation_required: true,
                reviewer_alias_sha256: format!("{:x}", Sha256::digest(b"local-owner-reviewer")),
                reviewed_at_utc: "2026-08-10T20:00:00Z".to_owned(),
            },
            BYTES,
            account_alias,
        )
        .unwrap()
    }

    fn ratified_competitive_pass_v1(
        event_kind: MtgoCompetitiveEventKindV1,
    ) -> RatifiedMtgoCompetitiveDuelPassAuthorizationV1 {
        let scope = competitive_scope_v1("UnbuckledPie", event_kind);
        let mode_commitment =
            validate_competitive_duel_pass_authorization_v1(&scope, "UnbuckledPie", event_kind)
                .unwrap();
        let expected = competitive_duel_pass_authorization_commitment_v1(
            &scope,
            "UnbuckledPie",
            event_kind,
            &mode_commitment,
        );
        ratify_competitive_duel_pass_authorization_with_commitment_v1(
            scope,
            "UnbuckledPie".to_owned(),
            event_kind,
            Some(&expected),
        )
        .unwrap()
    }

    fn competitive_match_authorization_v1(
        event_kind: MtgoCompetitiveEventKindV1,
        game_number: u8,
    ) -> MtgoCompetitiveMatchGameplayAuthorizationV1 {
        MtgoCompetitiveMatchGameplayAuthorizationV1 {
            schema_version: MTGO_COMPETITIVE_MATCH_GAMEPLAY_AUTHORIZATION_SCHEMA_V1,
            account_alias_sha256: format!("{:x}", Sha256::digest("UnbuckledPie".as_bytes())),
            written_permission_sha256: "b".repeat(64),
            event_kind,
            event_identity_sha256: "c".repeat(64),
            match_identity_sha256: "d".repeat(64),
            game_number,
            entry_authorization_sha256: "e".repeat(64),
            owner_launch_authorization_sha256: "f".repeat(64),
            exact_match_gameplay_authorized: true,
            valid_through_frame_sequence: 100,
        }
    }

    fn attended_match_launch_request_v4(
        event_kind: MtgoCompetitiveEventKindV1,
        game_number: u8,
        observed_frame_sequence: u64,
    ) -> MtgoAttendedCompetitiveMatchLaunchRequestV4 {
        MtgoAttendedCompetitiveMatchLaunchRequestV4 {
            schema_version: MTGO_ATTENDED_COMPETITIVE_MATCH_LAUNCH_REQUEST_SCHEMA_V4,
            event_kind,
            event_display_label: "Modern League".to_owned(),
            opponent_display_name: "VisibleOpponent".to_owned(),
            visible_match_id: "123456".to_owned(),
            visible_game_id: "789012".to_owned(),
            event_identity_sha256: "c".repeat(64),
            match_identity_sha256: "d".repeat(64),
            game_number,
            entry_authorization_sha256: "e".repeat(64),
            observed_frame_sequence,
            source_capture_commitment_sha256: "1".repeat(64),
            source_perception_result_commitment_sha256: "2".repeat(64),
            source_lifecycle_snapshot_commitment_sha256: "3".repeat(64),
            source_window_title_sha256: "4".repeat(64),
            source_event_label_region_sha256: "5".repeat(64),
            source_launch_identity_commitment_sha256: "6".repeat(64),
        }
    }

    fn ratified_match_launch_v1(
        event_kind: MtgoCompetitiveEventKindV1,
        game_number: u8,
    ) -> RatifiedMtgoCompetitiveMatchLaunchV1 {
        let scope = competitive_scope_v1("UnbuckledPie", event_kind);
        let authorization = competitive_match_authorization_v1(event_kind, game_number);
        let mode_commitment =
            validate_competitive_duel_pass_authorization_v1(&scope, "UnbuckledPie", event_kind)
                .unwrap();
        let gameplay_commitment =
            competitive_match_gameplay_authorization_commitment_v1(&authorization).unwrap();
        let expected = competitive_match_launch_commitment_v1(
            "UnbuckledPie",
            &mode_commitment,
            &gameplay_commitment,
            &authorization,
        );
        ratify_competitive_match_launch_with_commitment_v1(
            &scope,
            "UnbuckledPie",
            authorization,
            Some(&expected),
        )
        .unwrap()
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
    fn production_competitive_pass_ratification_is_empty_but_exact_modes_can_be_reviewed() {
        for event_kind in [
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ] {
            let scope = competitive_scope_v1("UnbuckledPie", event_kind);
            assert!(ratify_competitive_duel_pass_authorization_v1(
                scope.clone(),
                "UnbuckledPie".to_owned(),
                event_kind,
            )
            .is_err());

            let mode_commitment =
                validate_competitive_duel_pass_authorization_v1(&scope, "UnbuckledPie", event_kind)
                    .unwrap();
            let expected = competitive_duel_pass_authorization_commitment_v1(
                &scope,
                "UnbuckledPie",
                event_kind,
                &mode_commitment,
            );
            let ratified = ratify_competitive_duel_pass_authorization_with_commitment_v1(
                scope,
                "UnbuckledPie".to_owned(),
                event_kind,
                Some(&expected),
            )
            .unwrap();
            assert_eq!(ratified.event_kind_v1(), event_kind);
            assert_eq!(
                ratified.mode_authorization_commitment_sha256_v1(),
                mode_commitment
            );
            assert_eq!(ratified.authorization_commitment_sha256_v1(), expected);
            assert!(!ratified.safe_for_input_v1());
            assert!(!ratified.permits_event_entry_v1());
        }
    }

    #[test]
    fn reviewed_correspondence_ratification_binds_exact_review_and_one_mode() {
        for event_kind in [
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ] {
            let correspondence = checked_competitive_correspondence_v2();
            let review_commitment = correspondence.review_commitment_sha256().to_owned();
            let scope = correspondence
                .checked_untrusted_scope_for_mode_v1(event_kind)
                .unwrap();
            let mode_commitment =
                validate_competitive_duel_pass_authorization_v1(&scope, "UnbuckledPie", event_kind)
                    .unwrap();
            let expected = competitive_duel_pass_authorization_from_review_commitment_v2(
                &scope,
                "UnbuckledPie",
                event_kind,
                &mode_commitment,
                &review_commitment,
            );
            let candidate =
                review_competitive_duel_pass_ratification_candidate_from_correspondence_v2(
                    &correspondence,
                    "UnbuckledPie",
                    event_kind,
                )
                .unwrap();
            assert_eq!(
                candidate.permission_review_commitment_sha256,
                review_commitment
            );
            assert_eq!(
                candidate.mode_authorization_commitment_sha256,
                mode_commitment
            );
            assert_eq!(candidate.ratification_commitment_sha256, expected);
            assert!(!candidate.safe_for_live_input_v2());
            assert!(!candidate.permits_event_entry_v2());
            assert!(!candidate.permits_spending_v2());
            let ratified =
                ratify_competitive_duel_pass_authorization_from_correspondence_with_commitment_v2(
                    correspondence,
                    "UnbuckledPie".to_owned(),
                    event_kind,
                    Some(&expected),
                )
                .unwrap();
            assert_eq!(ratified.event_kind_v1(), event_kind);
            assert_eq!(
                ratified.permission_review_commitment_sha256_v2(),
                Some(review_commitment.as_str())
            );
            assert_eq!(
                ratified.mode_authorization_commitment_sha256_v1(),
                mode_commitment
            );
            assert_ne!(
                ratified.authorization_commitment_sha256_v1(),
                competitive_duel_pass_authorization_commitment_v1(
                    &scope,
                    "UnbuckledPie",
                    event_kind,
                    &mode_commitment,
                )
            );
            assert!(!ratified.permits_event_entry_v1());
            assert!(!ratified.safe_for_input_v1());
        }

        assert!(
            ratify_competitive_duel_pass_authorization_from_correspondence_v2(
                checked_competitive_correspondence_v2(),
                "UnbuckledPie".to_owned(),
                MtgoCompetitiveEventKindV1::League,
            )
            .is_err()
        );
        assert!(
            review_competitive_duel_pass_ratification_candidate_from_correspondence_v2(
                &checked_competitive_correspondence_v2(),
                "DifferentAccount",
                MtgoCompetitiveEventKindV1::League,
            )
            .is_err()
        );
    }

    #[test]
    fn production_exact_match_launch_ratification_is_independently_empty() {
        let scope = competitive_scope_v1("UnbuckledPie", MtgoCompetitiveEventKindV1::League);
        let authorization =
            competitive_match_authorization_v1(MtgoCompetitiveEventKindV1::League, 2);
        assert!(
            ratify_competitive_match_launch_v1(&scope, "UnbuckledPie", authorization,).is_err()
        );

        let ratified = ratified_match_launch_v1(MtgoCompetitiveEventKindV1::League, 2);
        assert_eq!(ratified.event_kind_v1(), MtgoCompetitiveEventKindV1::League);
        assert_eq!(ratified.game_number_v1(), 2);
        assert_eq!(
            ratified.launch_authorization_commitment_sha256_v1().len(),
            64
        );
        assert!(!ratified.permits_event_entry_v1());
    }

    #[test]
    fn attended_match_launch_binds_terminal_challenge_mode_game_and_frame_lifetime() {
        for event_kind in [
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ] {
            let scope = competitive_scope_v1("UnbuckledPie", event_kind);
            let mut request = attended_match_launch_request_v4(event_kind, 2, 40);
            request.event_display_label = match event_kind {
                MtgoCompetitiveEventKindV1::League => "Modern League".to_owned(),
                MtgoCompetitiveEventKindV1::Challenge => "Modern Challenge".to_owned(),
            };
            let nonce = [0xabu8; 8];
            let phrase =
                attended_competitive_match_launch_confirmation_phrase_v4(event_kind, 2, &nonce);
            let ratified = ratify_competitive_match_launch_from_attended_confirmation_v4(
                &scope,
                "UnbuckledPie",
                request.clone(),
                nonce,
                1_777,
                &phrase,
            )
            .unwrap();
            let authorization = ratified.gameplay_authorization_record_v2();
            assert_eq!(ratified.event_kind_v1(), event_kind);
            assert_eq!(ratified.game_number_v1(), 2);
            assert_eq!(ratified.valid_from_frame_sequence_v2(), 40);
            assert_eq!(
                ratified.valid_through_frame_sequence_v1(),
                40 + ATTENDED_COMPETITIVE_MATCH_MAX_FRAME_ADVANCE_V4
            );
            assert_eq!(
                ratified.owner_launch_authorization_sha256_v2(),
                authorization.owner_launch_authorization_sha256
            );
            assert_eq!(
                authorization.account_alias_sha256,
                scope.account_alias_sha256
            );
            assert_eq!(
                authorization.written_permission_sha256,
                scope.written_permission_sha256
            );
            assert!(authorization.exact_match_gameplay_authorized);
            assert!(!ratified.permits_event_entry_v1());

            let mut relabeled_request = request.clone();
            relabeled_request.opponent_display_name = "OtherVisibleOpponent".to_owned();
            let relabeled = ratify_competitive_match_launch_from_attended_confirmation_v4(
                &scope,
                "UnbuckledPie",
                relabeled_request,
                nonce,
                1_777,
                &phrase,
            )
            .unwrap();
            assert_ne!(
                relabeled.owner_launch_authorization_sha256_v2(),
                ratified.owner_launch_authorization_sha256_v2()
            );

            assert!(
                ratify_competitive_match_launch_from_attended_confirmation_v4(
                    &scope,
                    "UnbuckledPie",
                    request,
                    nonce,
                    1_777,
                    "AUTHORIZE SOMETHING ELSE",
                )
                .is_err()
            );
        }
    }

    #[test]
    fn attended_match_launch_cannot_authorize_a_prelaunch_frame() {
        let event_kind = MtgoCompetitiveEventKindV1::League;
        let scope = competitive_scope_v1("UnbuckledPie", event_kind);
        let authorization = ratified_competitive_pass_v1(event_kind);
        let request = attended_match_launch_request_v4(event_kind, 2, 40);
        let nonce = [0x31u8; 8];
        let phrase =
            attended_competitive_match_launch_confirmation_phrase_v4(event_kind, 2, &nonce);
        let match_launch = ratify_competitive_match_launch_from_attended_confirmation_v4(
            &scope,
            "UnbuckledPie",
            request,
            nonce,
            1_888,
            &phrase,
        )
        .unwrap();
        let mut prepared = MtgoOpaqueCompetitiveDuelPassPreparationCommitmentsV1 {
            competitive_action_plan_commitment_sha256: "1".repeat(64),
            competitive_mode_authorization_commitment_sha256: authorization
                .mode_authorization_commitment_sha256_v1()
                .to_owned(),
            competitive_match_gameplay_authorization_commitment_sha256: match_launch
                .gameplay_authorization_commitment_sha256_v1()
                .to_owned(),
            before_input_postcondition_verification_commitment_sha256: "6".repeat(64),
            immediate_capture_commitment_sha256: "2".repeat(64),
            immediate_perception_result_commitment_sha256: "3".repeat(64),
            preparation_commitment_sha256: "4".repeat(64),
            event_kind,
            game_number: 2,
            immediate_frame_id: 11,
            immediate_frame_sequence: 39,
            immediate_captured_at_unix_millis: 13,
        };
        let session = begin_competitive_game_session_v1(authorization, match_launch).unwrap();
        assert!(
            competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session,)
                .is_err()
        );
        prepared.immediate_frame_sequence = 40;
        assert!(
            competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session,)
                .is_ok()
        );
        prepared.immediate_frame_sequence =
            40 + ATTENDED_COMPETITIVE_MATCH_MAX_FRAME_ADVANCE_V4 + 1;
        assert!(
            competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session,)
                .is_err()
        );
    }

    #[test]
    fn attended_match_launch_rejects_broad_or_malformed_requests() {
        let event_kind = MtgoCompetitiveEventKindV1::League;
        let scope = competitive_scope_v1("UnbuckledPie", event_kind);
        let valid = attended_match_launch_request_v4(event_kind, 1, 40);

        let mut broad_scope = scope.clone();
        broad_scope.challenge_input = true;
        assert!(validate_attended_competitive_match_launch_request_v4(
            &broad_scope,
            "UnbuckledPie",
            &valid,
        )
        .is_err());
        assert!(validate_attended_competitive_match_launch_request_v4(
            &scope,
            "DifferentAccount",
            &valid,
        )
        .is_err());
        assert!(validate_attended_launch_display_label_v4(
            "\u{202e}spoof",
            64,
            "opponent display name"
        )
        .is_err());

        for malformed in [
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                schema_version: 1,
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                game_number: 0,
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                observed_frame_sequence: 0,
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                event_identity_sha256: "C".repeat(64),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                match_identity_sha256: valid.event_identity_sha256.clone(),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                event_display_label: String::new(),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                event_display_label: " Modern League".to_owned(),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                opponent_display_name: "UnbuckledPie".to_owned(),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                opponent_display_name: "Visible\nOpponent".to_owned(),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                opponent_display_name: "x".repeat(65),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                visible_match_id: "match-123".to_owned(),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                visible_game_id: String::new(),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                source_launch_identity_commitment_sha256: "A".repeat(64),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                observed_frame_sequence: u64::MAX,
                ..valid
            },
        ] {
            assert!(validate_attended_competitive_match_launch_request_v4(
                &scope,
                "UnbuckledPie",
                &malformed,
            )
            .is_err());
        }
    }

    #[test]
    fn competitive_pass_ratification_rejects_alias_mode_and_scope_broadening() {
        let league = competitive_scope_v1("UnbuckledPie", MtgoCompetitiveEventKindV1::League);
        assert!(validate_competitive_duel_pass_authorization_v1(
            &league,
            "another-account",
            MtgoCompetitiveEventKindV1::League,
        )
        .is_err());
        assert!(validate_competitive_duel_pass_authorization_v1(
            &league,
            "UnbuckledPie",
            MtgoCompetitiveEventKindV1::Challenge,
        )
        .is_err());

        for mutation in 0..4 {
            let mut broader = league.clone();
            match mutation {
                0 => broader.challenge_input = true,
                1 => broader.private_match_input = true,
                2 => broader.open_play_input = true,
                3 => broader.other_prize_event_input = true,
                _ => unreachable!(),
            }
            assert!(validate_competitive_duel_pass_authorization_v1(
                &broader,
                "UnbuckledPie",
                MtgoCompetitiveEventKindV1::League,
            )
            .is_err());
        }

        let challenge = competitive_scope_v1("UnbuckledPie", MtgoCompetitiveEventKindV1::Challenge);
        let league_mode = validate_competitive_duel_pass_authorization_v1(
            &league,
            "UnbuckledPie",
            MtgoCompetitiveEventKindV1::League,
        )
        .unwrap();
        let challenge_mode = validate_competitive_duel_pass_authorization_v1(
            &challenge,
            "UnbuckledPie",
            MtgoCompetitiveEventKindV1::Challenge,
        )
        .unwrap();
        assert_ne!(league_mode, challenge_mode);
    }

    #[test]
    fn prepared_pass_binds_only_to_the_same_ratified_competitive_mode() {
        let authorization = ratified_competitive_pass_v1(MtgoCompetitiveEventKindV1::League);
        let match_launch = ratified_match_launch_v1(MtgoCompetitiveEventKindV1::League, 2);
        let correct_mode_commitment = authorization
            .mode_authorization_commitment_sha256_v1()
            .to_owned();
        let mut prepared = MtgoOpaqueCompetitiveDuelPassPreparationCommitmentsV1 {
            competitive_action_plan_commitment_sha256: "1".repeat(64),
            competitive_mode_authorization_commitment_sha256: authorization
                .mode_authorization_commitment_sha256_v1()
                .to_owned(),
            competitive_match_gameplay_authorization_commitment_sha256: match_launch
                .gameplay_authorization_commitment_sha256_v1()
                .to_owned(),
            before_input_postcondition_verification_commitment_sha256: "6".repeat(64),
            immediate_capture_commitment_sha256: "2".repeat(64),
            immediate_perception_result_commitment_sha256: "3".repeat(64),
            preparation_commitment_sha256: "4".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 2,
            immediate_frame_id: 11,
            immediate_frame_sequence: 12,
            immediate_captured_at_unix_millis: 13,
        };
        let session = begin_competitive_game_session_v1(authorization, match_launch).unwrap();
        let bound = competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session)
            .unwrap();
        assert_eq!(bound.event_kind, MtgoCompetitiveEventKindV1::League);
        assert_eq!(bound.game_number, 2);
        assert_eq!(bound.immediate_frame_id, 11);
        assert_eq!(bound.immediate_frame_sequence, 12);
        assert_eq!(bound.authorization_binding_commitment_sha256.len(), 64);

        prepared.event_kind = MtgoCompetitiveEventKindV1::Challenge;
        assert!(
            competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session,)
                .is_err()
        );
        prepared.event_kind = MtgoCompetitiveEventKindV1::League;
        prepared.competitive_mode_authorization_commitment_sha256 = "5".repeat(64);
        assert!(
            competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session,)
                .is_err()
        );
        prepared.competitive_mode_authorization_commitment_sha256 = correct_mode_commitment;
        prepared.competitive_match_gameplay_authorization_commitment_sha256 = "8".repeat(64);
        assert!(
            competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session,)
                .is_err()
        );
    }

    #[test]
    fn competitive_game_session_advances_only_after_a_newer_visible_transition() {
        let authorization = ratified_competitive_pass_v1(MtgoCompetitiveEventKindV1::League);
        let match_launch = ratified_match_launch_v1(MtgoCompetitiveEventKindV1::League, 2);
        let session = begin_competitive_game_session_v1(authorization, match_launch).unwrap();
        let initial = session.commitments_v1();
        assert_eq!(initial.confirmed_action_count, 0);
        assert_eq!(initial.last_confirmed_frame_sequence, 0);
        assert!(!session.safe_for_input_v1());
        assert!(!session.permits_event_entry_v1());

        let visible = MtgoOpaqueCompetitiveDuelPassConfirmationCommitmentsV1 {
            before_input_verification_commitment_sha256: "3".repeat(64),
            after_capture_commitment_sha256: "4".repeat(64),
            checked_postcondition_commitment_sha256: "5".repeat(64),
            opaque_confirmation_commitment_sha256: "6".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 2,
            after_frame_id: 10,
            after_frame_sequence: 12,
            postcondition_candidate_count: 3,
        };
        let session =
            advance_competitive_game_session_v1(session, &visible, &"a".repeat(64)).unwrap();
        let advanced = session.commitments_v1();
        assert_eq!(advanced.confirmed_action_count, 1);
        assert_eq!(advanced.last_confirmed_frame_sequence, 12);
        assert_ne!(
            initial.session_commitment_sha256,
            advanced.session_commitment_sha256
        );

        let mut prepared = MtgoOpaqueCompetitiveDuelPassPreparationCommitmentsV1 {
            competitive_action_plan_commitment_sha256: "1".repeat(64),
            competitive_mode_authorization_commitment_sha256: session
                .authorization
                .mode_authorization_commitment_sha256
                .clone(),
            competitive_match_gameplay_authorization_commitment_sha256: session
                .match_launch
                .gameplay_authorization_commitment_sha256
                .clone(),
            before_input_postcondition_verification_commitment_sha256: "6".repeat(64),
            immediate_capture_commitment_sha256: "2".repeat(64),
            immediate_perception_result_commitment_sha256: "3".repeat(64),
            preparation_commitment_sha256: "4".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 2,
            immediate_frame_id: 11,
            immediate_frame_sequence: 12,
            immediate_captured_at_unix_millis: 13,
        };
        assert!(
            competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session)
                .is_err()
        );
        prepared.immediate_frame_sequence = 13;
        let next = competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session)
            .unwrap();
        assert_eq!(
            next.competitive_game_session_commitment_sha256,
            advanced.session_commitment_sha256
        );
    }

    #[test]
    fn competitive_game_session_rejects_mismatched_or_expired_authority() {
        let league = ratified_competitive_pass_v1(MtgoCompetitiveEventKindV1::League);
        let challenge_launch = ratified_match_launch_v1(MtgoCompetitiveEventKindV1::Challenge, 1);
        assert!(begin_competitive_game_session_v1(league, challenge_launch).is_err());

        let league = ratified_competitive_pass_v1(MtgoCompetitiveEventKindV1::League);
        let mut invalid_launch = ratified_match_launch_v1(MtgoCompetitiveEventKindV1::League, 1);
        invalid_launch.valid_from_frame_sequence = 0;
        assert!(begin_competitive_game_session_v1(league, invalid_launch).is_err());

        let league = ratified_competitive_pass_v1(MtgoCompetitiveEventKindV1::League);
        let launch = ratified_match_launch_v1(MtgoCompetitiveEventKindV1::League, 1);
        let session = begin_competitive_game_session_v1(league, launch).unwrap();
        let visible = MtgoOpaqueCompetitiveDuelPassConfirmationCommitmentsV1 {
            before_input_verification_commitment_sha256: "3".repeat(64),
            after_capture_commitment_sha256: "4".repeat(64),
            checked_postcondition_commitment_sha256: "5".repeat(64),
            opaque_confirmation_commitment_sha256: "6".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 1,
            after_frame_id: 101,
            after_frame_sequence: 101,
            postcondition_candidate_count: 1,
        };
        assert!(advance_competitive_game_session_v1(session, &visible, &"a".repeat(64)).is_err());
    }

    #[test]
    fn competitive_transition_receipt_binds_input_authority_visible_result_and_frame() {
        let visible = MtgoOpaqueCompetitiveDuelPassConfirmationCommitmentsV1 {
            before_input_verification_commitment_sha256: "3".repeat(64),
            after_capture_commitment_sha256: "4".repeat(64),
            checked_postcondition_commitment_sha256: "5".repeat(64),
            opaque_confirmation_commitment_sha256: "6".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 1,
            after_frame_id: 10,
            after_frame_sequence: 11,
            postcondition_candidate_count: 3,
        };
        let baseline =
            competitive_duel_pass_transition_receipt_v1(&"1".repeat(64), &"2".repeat(64), &visible);
        assert_eq!(baseline.len(), 64);
        assert_ne!(
            baseline,
            competitive_duel_pass_transition_receipt_v1(&"a".repeat(64), &"2".repeat(64), &visible,)
        );
        let mut changed = visible.clone();
        changed.event_kind = MtgoCompetitiveEventKindV1::Challenge;
        assert_ne!(
            baseline,
            competitive_duel_pass_transition_receipt_v1(&"1".repeat(64), &"2".repeat(64), &changed,)
        );
        changed.event_kind = MtgoCompetitiveEventKindV1::League;
        changed.after_frame_sequence = 12;
        assert_ne!(
            baseline,
            competitive_duel_pass_transition_receipt_v1(&"1".repeat(64), &"2".repeat(64), &changed,)
        );
        changed.after_frame_sequence = 11;
        changed.postcondition_candidate_count = 4;
        assert_ne!(
            baseline,
            competitive_duel_pass_transition_receipt_v1(&"1".repeat(64), &"2".repeat(64), &changed,)
        );
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
