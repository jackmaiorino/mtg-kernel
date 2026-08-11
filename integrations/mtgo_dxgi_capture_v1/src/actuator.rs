use crate::probe::{
    capture_admitted_mtgo_competitive_navigation_frame_v1,
    classify_admitted_mtgo_competitive_navigation_frame_v1,
    confirm_opaque_competitive_duel_pass_postcondition_v1,
    confirm_opaque_competitive_entry_postcondition_v1,
    confirm_pregame_keep_to_bottom_six_transition_v3,
    confirm_pregame_keep_to_first_main_transition_v3, confirm_pregame_mulligan_transition_v3,
    prepare_opaque_competitive_duel_gesture_source_stage_from_pinned_runtime_v1,
    prepare_pregame_actuation_v3, resolve_competitive_entry_pointer_target_v1,
    validate_classifier_backed_competitive_entry_frame_transition_v1,
    validate_classifier_backed_competitive_entry_immediate_recapture_v1,
    MtgoCompetitiveEntryControlDryRunPartsV1, MtgoCompetitiveEntryFrameTransitionCommitmentsV1,
    MtgoCompetitiveEntryImmediateRecaptureCommitmentsV1, MtgoCompetitiveEntryPointerTargetV1,
    MtgoCompetitiveEntryVisibleConfirmationCommitmentsV1, MtgoCompetitiveNavigationFrameIdentityV1,
    MtgoOpaqueCompetitiveDuelGestureSequenceCommitmentsV1,
    MtgoOpaqueCompetitiveDuelGestureSourcePreparationCommitmentsV1,
    MtgoOpaqueCompetitiveDuelPassConfirmationCommitmentsV1,
    MtgoOpaqueCompetitiveDuelPassPreparationCommitmentsV1,
    MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1,
    MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1, MtgoPlannedPregamePostconditionV3,
    OpaqueMtgoAdmittedDuelPerceptionV1, OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    OpaqueMtgoCompetitiveDuelGestureSequenceV1, OpaqueMtgoCompetitiveEntryControlDryRunV1,
    OpaqueMtgoCompetitiveEntryReviewIdentityV1, OpaqueMtgoCompetitiveLaunchIdentityV1,
    OpaqueMtgoConfirmedCompetitiveDuelPassV1, OpaqueMtgoConfirmedCompetitiveEntryPostconditionV1,
    OpaqueMtgoConfirmedKeepToBottomSixTransitionV3, OpaqueMtgoConfirmedKeepToFirstMainTransitionV3,
    OpaqueMtgoConfirmedMulliganTransitionV3, OpaqueMtgoDxgiBottomSixInitialMeasurementV3,
    OpaqueMtgoDxgiFirstMainMeasurementV3, OpaqueMtgoDxgiMulliganMeasurementV3,
    OpaqueMtgoPregameActionPlanV3,
    OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1 as ProbeOpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1,
    OpaqueMtgoPreparedCompetitiveDuelPassV1,
    OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1, PreparedPregameActuationV3,
};
use mtgo_blackbox_v1::{
    canonical_duel_gesture_action_families_v1,
    competitive_match_gameplay_authorization_commitment_v1,
    competitive_mode_authorization_commitment_v1, make_offline_competitive_lifecycle_intent_v1,
    validate_authorization_for_mode_v1, AdmittedMtgoCompetitiveNavigationProfileV1,
    AdmittedMtgoDuelGestureProfileV1, AdmittedMtgoDuelPerceptionProfileV1,
    CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1, MtgoAuthorizationScopeV1,
    MtgoCompetitiveEntryAuthorizationV1, MtgoCompetitiveEntryResourceV1,
    MtgoCompetitiveEntryTermsV1, MtgoCompetitiveEventKindV1, MtgoCompetitiveLifecycleActionV1,
    MtgoCompetitiveLifecyclePhaseV1, MtgoCompetitiveMatchGameplayAuthorizationV1,
    MtgoDuelActionFamilyV1, MtgoPregameActionSemanticV1, MtgoRuntimeModeV1,
    MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1, MTGO_COMPETITIVE_MATCH_GAMEPLAY_AUTHORIZATION_SCHEMA_V1,
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
const COMPETITIVE_DUEL_GESTURE_AUTHORIZATION_FROM_REVIEW_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-duel-gesture-authorization-from-review-v1";
const RATIFIED_COMPETITIVE_DUEL_GESTURE_AUTHORIZATION_FROM_REVIEW_COMMITMENT_V1: Option<&str> =
    None;
const COMPETITIVE_ENTRY_AUTHORIZATION_RATIFICATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-entry-authorization-ratification-v1";
const RATIFIED_COMPETITIVE_ENTRY_AUTHORIZATION_COMMITMENT_V1: Option<&str> = None;
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
const ATTENDED_COMPETITIVE_GESTURE_MATCH_LAUNCH_UPGRADE_DOMAIN_V1: &[u8] =
    b"mtgo-attended-competitive-gesture-match-launch-upgrade-v1";
const COMPETITIVE_GESTURE_GAME_SESSION_INITIAL_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-gesture-game-session-initial-v1";
const COMPETITIVE_GESTURE_SESSION_SEQUENCE_BINDING_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-gesture-session-sequence-binding-v1";
const COMPETITIVE_GESTURE_SESSION_SOURCE_PREPARATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-gesture-session-source-preparation-v1";
const ATTENDED_COMPETITIVE_ENTRY_REVIEW_REQUEST_DOMAIN_V1: &[u8] =
    b"mtgo-attended-competitive-entry-review-request-v1";
const ATTENDED_COMPETITIVE_ENTRY_REVIEW_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-attended-competitive-entry-review-receipt-v1";
const CLASSIFIER_BOUND_COMPETITIVE_ENTRY_REVIEW_DOMAIN_V3: &[u8] =
    b"mtgo-classifier-bound-competitive-entry-review-v3";
const CONTROL_BOUND_COMPETITIVE_ENTRY_REVIEW_DOMAIN_V4: &[u8] =
    b"mtgo-control-bound-competitive-entry-review-v4";
const COMPETITIVE_ENTRY_POSTCONDITION_DRY_RUN_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-entry-postcondition-dry-run-v1";
const COMPETITIVE_ENTRY_PREPARATION_DOMAIN_V1: &[u8] = b"mtgo-competitive-entry-preparation-v1";
const COMPETITIVE_ENTRY_INPUT_RECEIPT_DOMAIN_V1: &[u8] = b"mtgo-competitive-entry-input-receipt-v1";
const COMPETITIVE_ENTRY_CONFIRMATION_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-entry-confirmation-receipt-v1";
const ATTENDED_COMPETITIVE_MATCH_MAX_FRAME_ADVANCE_V4: u64 = 512;

const MTGO_ATTENDED_COMPETITIVE_MATCH_LAUNCH_REQUEST_SCHEMA_V4: u32 = 4;
const MTGO_ATTENDED_COMPETITIVE_ENTRY_REVIEW_SCHEMA_V1: u32 = 1;

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

/// One separately ratified exact-account permission for the complete reviewed
/// duel gesture profile in exactly one League or Challenge mode. The profile
/// identity is retained, but this value is not an input command and grants no
/// event-entry or spending authority. The production ratification root is
/// empty.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::RatifiedMtgoCompetitiveDuelGestureAuthorizationV1;
/// let _forged = RatifiedMtgoCompetitiveDuelGestureAuthorizationV1 {};
/// ```
pub struct RatifiedMtgoCompetitiveDuelGestureAuthorizationV1 {
    scope: MtgoAuthorizationScopeV1,
    _permission_correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    _gesture_profile: AdmittedMtgoDuelGestureProfileV1,
    #[allow(dead_code)]
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
    mode_authorization_commitment_sha256: String,
    permission_review_commitment_sha256: String,
    gesture_evaluation_commitment_sha256: String,
    gesture_profile_admission_commitment_sha256: String,
    authorization_commitment_sha256: String,
}

impl RatifiedMtgoCompetitiveDuelGestureAuthorizationV1 {
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

    pub fn permission_review_commitment_sha256_v1(&self) -> &str {
        &self.permission_review_commitment_sha256
    }

    pub fn gesture_evaluation_commitment_sha256_v1(&self) -> &str {
        &self.gesture_evaluation_commitment_sha256
    }

    pub fn gesture_profile_admission_commitment_sha256_v1(&self) -> &str {
        &self.gesture_profile_admission_commitment_sha256
    }

    pub fn authorization_commitment_sha256_v1(&self) -> &str {
        &self.authorization_commitment_sha256
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoReviewedCompetitiveGestureRatificationCandidateV1 {
    pub permission_review_commitment_sha256: String,
    pub account_alias_sha256: String,
    pub correspondence_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub gesture_evaluation_commitment_sha256: String,
    pub gesture_profile_admission_commitment_sha256: String,
    pub gesture_target_runtime_binary_sha256: String,
    pub gesture_target_assets_manifest_sha256: String,
    pub supported_action_families: Vec<MtgoDuelActionFamilyV1>,
    pub ratification_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
}

impl MtgoReviewedCompetitiveGestureRatificationCandidateV1 {
    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy)]
struct CompetitiveDuelGestureProfileFactsV1<'a> {
    evaluation_commitment_sha256: &'a str,
    admission_commitment_sha256: &'a str,
    runtime_binary_sha256: &'a str,
    assets_manifest_sha256: &'a str,
    supported_action_families: &'a [MtgoDuelActionFamilyV1],
}

/// Copyable review telemetry for one exact League or Challenge entry and its
/// exact existing-account resource terms. This is a proposed compile-time
/// ratification value only. It cannot enter the event, spend resources, or
/// send input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoReviewedCompetitiveEntryRatificationCandidateV1 {
    pub permission_review_commitment_sha256: String,
    pub account_alias_sha256: String,
    pub correspondence_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub control_bound_review_commitment_sha256: String,
    pub owner_review_receipt_sha256: String,
    pub entry_authorization_sha256: String,
    pub source_identity_commitment_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub source_navigation_classification_result_commitment_sha256: String,
    pub visible_control_region_sha256: String,
    pub event_identity_sha256: String,
    pub entry_terms_sha256: String,
    pub ratification_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
}

impl MtgoReviewedCompetitiveEntryRatificationCandidateV1 {
    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Separately compile-ratified authority for one exact owner-reviewed entry.
/// It owns the checked correspondence and the complete control-bound review,
/// but it exposes no coordinates or input method. A future actuator must also
/// require an immediate recapture and the exact visible postcondition gate.
///
/// The production ratification root is empty.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::RatifiedMtgoCompetitiveEntryAuthorizationV1;
/// let _forged = RatifiedMtgoCompetitiveEntryAuthorizationV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::RatifiedMtgoCompetitiveEntryAuthorizationV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<RatifiedMtgoCompetitiveEntryAuthorizationV1>();
/// ```
pub struct RatifiedMtgoCompetitiveEntryAuthorizationV1 {
    _permission_correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    _review: CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4,
    _scope: MtgoAuthorizationScopeV1,
    #[allow(dead_code)]
    visible_account_alias: String,
    commitments: MtgoReviewedCompetitiveEntryRatificationCandidateV1,
}

impl RatifiedMtgoCompetitiveEntryAuthorizationV1 {
    pub fn commitments_v1(&self) -> MtgoReviewedCompetitiveEntryRatificationCandidateV1 {
        self.commitments.clone()
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.commitments.event_kind
    }

    pub fn resource_v1(&self) -> MtgoCompetitiveEntryResourceV1 {
        self.commitments.resource
    }

    pub fn amount_v1(&self) -> u32 {
        self.commitments.amount
    }

    pub fn authorizes_exact_reviewed_entry_v1(&self) -> bool {
        true
    }

    pub fn authorizes_exact_reviewed_resource_terms_v1(&self) -> bool {
        true
    }

    pub fn permits_exact_reviewed_spending_v1(&self) -> bool {
        self.commitments.resource != MtgoCompetitiveEntryResourceV1::NoCost
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPreparedCompetitiveEntryCommitmentsV1 {
    pub entry_ratification_commitment_sha256: String,
    pub immediate_recapture: MtgoCompetitiveEntryImmediateRecaptureCommitmentsV1,
    pub preparation_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
    pub immediate_frame_id: u64,
    pub immediate_frame_sequence: u64,
}

/// One exact separately ratified entry whose visible Entry Review state was
/// reacquired and reclassified immediately before the exact entry input
/// boundary. The complete authorization, coordinate-private control, and fresh
/// opaque frame remain retained. Only `execute_prepared_competitive_entry_v1`
/// may consume it, and that production-disabled path permits exactly one click
/// before withholding all later input pending visible confirmation.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPreparedCompetitiveEntryV1;
/// let _forged = OpaqueMtgoPreparedCompetitiveEntryV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPreparedCompetitiveEntryV1;
/// fn cannot_control(value: &OpaqueMtgoPreparedCompetitiveEntryV1) {
///     let _ = value.target_point_client_px();
///     let _ = value.send_input();
///     let _ = value.enter_event();
/// }
/// ```
pub struct OpaqueMtgoPreparedCompetitiveEntryV1 {
    _authorization: RatifiedMtgoCompetitiveEntryAuthorizationV1,
    _immediate_frame: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    pointer_target: MtgoCompetitiveEntryPointerTargetV1,
    commitments: MtgoPreparedCompetitiveEntryCommitmentsV1,
}

impl OpaqueMtgoPreparedCompetitiveEntryV1 {
    pub fn commitments_v1(&self) -> MtgoPreparedCompetitiveEntryCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEntryInputReceiptCommitmentsV1 {
    pub preparation_commitment_sha256: String,
    pub entry_ratification_commitment_sha256: String,
    pub input_receipt_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
    pub immediate_frame_id: u64,
    pub immediate_frame_sequence: u64,
    pub input_sent_at_unix_millis: u128,
    pub cursor_parked_outside_client: bool,
}

/// Proof that one exact-entry click was emitted and the shared process gate is
/// withholding all later input pending a visible entered-waiting result.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPendingCompetitiveEntryV1;
/// let _forged = OpaqueMtgoPendingCompetitiveEntryV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPendingCompetitiveEntryV1;
/// fn cannot_repeat(value: &OpaqueMtgoPendingCompetitiveEntryV1) {
///     let _ = value.send_input();
///     let _ = value.enter_another_event();
///     let _ = value.spend_again();
/// }
/// ```
pub struct OpaqueMtgoPendingCompetitiveEntryV1 {
    prepared: OpaqueMtgoPreparedCompetitiveEntryV1,
    commitments: MtgoCompetitiveEntryInputReceiptCommitmentsV1,
}

impl OpaqueMtgoPendingCompetitiveEntryV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveEntryInputReceiptCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }

    pub fn permits_additional_spending_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoConfirmedCompetitiveEntryCommitmentsV1 {
    pub input_receipt_sha256: String,
    pub preparation_commitment_sha256: String,
    pub entry_ratification_commitment_sha256: String,
    pub visible_transition_commitment_sha256: String,
    pub visible_confirmation_commitment_sha256: String,
    pub confirmation_receipt_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub after_captured_at_unix_millis: u128,
    pub postcondition_candidate_count: u32,
}

/// One exact entry click whose strictly newer visible result is the matching
/// entered-and-waiting-for-pairing state. It has no authority for gameplay,
/// another entry, another resource spend, or another input.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoConfirmedCompetitiveEntryV1;
/// let _forged = OpaqueMtgoConfirmedCompetitiveEntryV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoConfirmedCompetitiveEntryV1;
/// fn cannot_continue(value: &OpaqueMtgoConfirmedCompetitiveEntryV1) {
///     let _ = value.send_input();
///     let _ = value.enter_another_event();
///     let _ = value.start_gameplay();
/// }
/// ```
pub struct OpaqueMtgoConfirmedCompetitiveEntryV1 {
    _authorization: RatifiedMtgoCompetitiveEntryAuthorizationV1,
    _visible_confirmation: OpaqueMtgoConfirmedCompetitiveEntryPostconditionV1,
    commitments: MtgoConfirmedCompetitiveEntryCommitmentsV1,
}

impl OpaqueMtgoConfirmedCompetitiveEntryV1 {
    pub fn commitments_v1(&self) -> MtgoConfirmedCompetitiveEntryCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn entry_visibly_confirmed_v1(&self) -> bool {
        true
    }

    pub fn safe_for_gameplay_input_v1(&self) -> bool {
        false
    }

    pub fn permits_additional_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_additional_spending_v1(&self) -> bool {
        false
    }
}

/// The exact coordinate-free facts shown to the account owner before they
/// review one visible League or Challenge entry. This request cannot enter the
/// event or spend the declared resources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoAttendedCompetitiveEntryReviewRequestV1 {
    schema_version: u32,
    event_kind: MtgoCompetitiveEventKindV1,
    event_display_label: String,
    source_lifecycle_snapshot_commitment_sha256: String,
    frame_id: u64,
    frame_sequence: u64,
    event_identity_sha256: String,
    entry_terms: MtgoCompetitiveEntryTermsV1,
    account_alias_sha256: String,
    correspondence_sha256: String,
    permission_review_commitment_sha256: String,
    mode_authorization_commitment_sha256: String,
}

struct MtgoAttendedCompetitiveEntryReviewPartsV1 {
    entry_authorization: MtgoCompetitiveEntryAuthorizationV1,
    commitments: MtgoAttendedCompetitiveEntryReviewCommitmentsV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoAttendedCompetitiveEntryReviewCommitmentsV1 {
    pub source_lifecycle_snapshot_commitment_sha256: String,
    pub permission_review_commitment_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub request_commitment_sha256: String,
    pub entry_authorization_sha256: String,
    pub owner_review_receipt_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
}

/// One owner-attended review of the exact event and entry terms reported by a
/// checked visible lifecycle snapshot. The snapshot remains checked-untrusted,
/// so this value deliberately grants no event-entry, spending, or input
/// authority. A future live entry seam must additionally bind an opaque,
/// admitted navigation capture to these same facts.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1;
/// let _forged = CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1>();
/// ```
pub struct CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1 {
    _source: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    entry_authorization: MtgoCompetitiveEntryAuthorizationV1,
    commitments: MtgoAttendedCompetitiveEntryReviewCommitmentsV1,
}

impl CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1 {
    pub fn commitments_v1(&self) -> MtgoAttendedCompetitiveEntryReviewCommitmentsV1 {
        self.commitments.clone()
    }

    /// Returns coordinate-free data for validating a future offline lifecycle
    /// intent. The record alone is not authority to enter an event.
    pub fn entry_authorization_record_v1(&self) -> MtgoCompetitiveEntryAuthorizationV1 {
        self.entry_authorization.clone()
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2 {
    pub source_identity_commitment_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub source_navigation_classification_result_commitment_sha256: Option<String>,
    pub attended_review: MtgoAttendedCompetitiveEntryReviewCommitmentsV1,
}

/// One attended entry review whose lifecycle interpretation has already been
/// bound to a retained opaque composed-desktop navigation frame. It remains
/// checked-untrusted and non-actionable because source binding does not prove
/// classifier accuracy or resolve the transient-occluder race.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2;
/// let _forged = CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2;
/// fn require_clone<T: Clone>() {}
/// require_clone::<CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2>();
/// ```
pub struct CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2 {
    _source_identity: OpaqueMtgoCompetitiveEntryReviewIdentityV1,
    entry_authorization: MtgoCompetitiveEntryAuthorizationV1,
    commitments: MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2,
}

impl CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2 {
    pub fn commitments_v2(&self) -> MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2 {
        self.commitments.clone()
    }

    pub fn entry_authorization_record_v2(&self) -> MtgoCompetitiveEntryAuthorizationV1 {
        self.entry_authorization.clone()
    }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3 {
    pub source_navigation_classification_result_commitment_sha256: String,
    pub classifier_bound_review_commitment_sha256: String,
    pub source_bound_review: MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2,
}

/// One attended entry review whose visible frame was interpreted by the exact
/// profile-pinned classifier retained inside the source identity. A stricter
/// control-bound v4 review adds the required visible Confirm Entry control.
///
/// It remains checked-untrusted and deliberately grants no Join, spending, or
/// input authority. The account owner must separately authorize any eventual
/// entry action and exact resource spend.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3;
/// let _forged = CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3;
/// fn require_clone<T: Clone>() {}
/// require_clone::<CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3>();
/// ```
pub struct CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3 {
    _source_bound_review: CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2,
    entry_authorization: MtgoCompetitiveEntryAuthorizationV1,
    commitments: MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3,
}

impl CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3 {
    pub fn commitments_v3(&self) -> MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3 {
        self.commitments.clone()
    }

    pub fn entry_authorization_record_v3(&self) -> MtgoCompetitiveEntryAuthorizationV1 {
        self.entry_authorization.clone()
    }

    pub fn safe_for_live_input_v3(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v3(&self) -> bool {
        false
    }

    pub fn permits_spending_v3(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoControlBoundCompetitiveEntryReviewCommitmentsV4 {
    pub control_bound_review_commitment_sha256: String,
    pub classifier_bound_review: MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3,
    pub entry_control_dry_run: MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1,
}

/// One owner-attended entry review bound to a coordinate-private, visibly
/// enabled Confirm Entry control on the exact classifier-backed source frame.
/// This is the most specific dry-run type and the only review type a future
/// event-entry actuator may accept.
///
/// It still grants no Join, spending, or input authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4;
/// let _forged = CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4;
/// fn require_clone<T: Clone>() {}
/// require_clone::<CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4>();
/// ```
pub struct CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4 {
    _classifier_bound_review: CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3,
    _visible_control_label: String,
    _control_rect_client_px: mtgo_blackbox_v1::MtgoRectPxV1,
    entry_authorization: MtgoCompetitiveEntryAuthorizationV1,
    commitments: MtgoControlBoundCompetitiveEntryReviewCommitmentsV4,
}

impl CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4 {
    pub fn commitments_v4(&self) -> MtgoControlBoundCompetitiveEntryReviewCommitmentsV4 {
        self.commitments.clone()
    }

    pub fn entry_authorization_record_v4(&self) -> MtgoCompetitiveEntryAuthorizationV1 {
        self.entry_authorization.clone()
    }

    pub fn safe_for_live_input_v4(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v4(&self) -> bool {
        false
    }

    pub fn permits_spending_v4(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEntryPostconditionDryRunCommitmentsV1 {
    pub control_bound_review_commitment_sha256: String,
    pub entry_authorization_sha256: String,
    pub frame_transition: MtgoCompetitiveEntryFrameTransitionCommitmentsV1,
    pub postcondition_dry_run_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
}

/// One exact classifier-backed entry-review frame paired with a strictly newer
/// entered-waiting frame from the same account, client process, window,
/// geometry, output, profile, classifier runtime, mode, and event identity.
///
/// This is calibration evidence only. No input occurred between the retained
/// frames, so the pair does not claim that a Join action caused the transition.
/// It exposes no pixels or coordinates and grants no entry, spending, or input
/// authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1;
/// let _forged = CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1>();
/// ```
pub struct CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1 {
    _source_review: CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4,
    _entered_waiting_frame: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    commitments: MtgoCompetitiveEntryPostconditionDryRunCommitmentsV1,
}

impl CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveEntryPostconditionDryRunCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn claims_action_causality_v1(&self) -> bool {
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

/// An attended extension of one exact priority-Pass match launch to the
/// complete reviewed eleven-family gesture profile. It owns both independent
/// ratifications and the original exact-game owner launch. It grants no event
/// entry or spending authority and has no input conversion.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::RatifiedMtgoCompetitiveGestureMatchLaunchV1;
/// let _forged = RatifiedMtgoCompetitiveGestureMatchLaunchV1 {};
/// ```
pub struct RatifiedMtgoCompetitiveGestureMatchLaunchV1 {
    gesture_authorization: RatifiedMtgoCompetitiveDuelGestureAuthorizationV1,
    pass_match_launch: RatifiedMtgoCompetitiveMatchLaunchV1,
    gesture_match_launch_commitment_sha256: String,
}

impl RatifiedMtgoCompetitiveGestureMatchLaunchV1 {
    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.pass_match_launch.authorization.event_kind
    }

    pub fn game_number_v1(&self) -> u8 {
        self.pass_match_launch.authorization.game_number
    }

    pub fn mode_authorization_commitment_sha256_v1(&self) -> &str {
        &self
            .gesture_authorization
            .mode_authorization_commitment_sha256
    }

    pub fn general_gesture_permission_commitment_sha256_v1(&self) -> &str {
        &self.gesture_authorization.authorization_commitment_sha256
    }

    pub fn pass_match_launch_commitment_sha256_v1(&self) -> &str {
        &self
            .pass_match_launch
            .launch_authorization_commitment_sha256
    }

    pub fn gesture_match_launch_commitment_sha256_v1(&self) -> &str {
        &self.gesture_match_launch_commitment_sha256
    }

    pub fn gesture_evaluation_commitment_sha256_v1(&self) -> &str {
        &self
            .gesture_authorization
            .gesture_evaluation_commitment_sha256
    }

    pub fn gesture_profile_admission_commitment_sha256_v1(&self) -> &str {
        &self
            .gesture_authorization
            .gesture_profile_admission_commitment_sha256
    }

    pub fn valid_from_frame_sequence_v1(&self) -> u64 {
        self.pass_match_launch.valid_from_frame_sequence
    }

    pub fn valid_through_frame_sequence_v1(&self) -> u64 {
        self.pass_match_launch
            .authorization
            .valid_through_frame_sequence
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveGestureGameSessionCommitmentsV1 {
    pub session_commitment_sha256: String,
    pub general_gesture_permission_commitment_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub pass_match_launch_commitment_sha256: String,
    pub match_gameplay_authorization_commitment_sha256: String,
    pub gesture_match_launch_commitment_sha256: String,
    pub gesture_evaluation_commitment_sha256: String,
    pub gesture_profile_admission_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub valid_from_frame_sequence: u64,
    pub valid_through_frame_sequence: u64,
    pub last_confirmed_frame_sequence: u64,
    pub confirmed_action_count: u64,
}

/// Move-only all-family session identity for one exact already-entered League
/// or Challenge game. This tranche creates no preparation, execution, or
/// advancement method. A future actuator must consume the session and return
/// it only after each exact visible postcondition.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveGestureGameSessionV1;
/// let _forged = OpaqueMtgoCompetitiveGestureGameSessionV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveGestureGameSessionV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveGestureGameSessionV1>();
/// ```
pub struct OpaqueMtgoCompetitiveGestureGameSessionV1 {
    launch: RatifiedMtgoCompetitiveGestureMatchLaunchV1,
    session_commitment_sha256: String,
    last_confirmed_frame_sequence: u64,
    confirmed_action_count: u64,
}

impl OpaqueMtgoCompetitiveGestureGameSessionV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveGestureGameSessionCommitmentsV1 {
        MtgoCompetitiveGestureGameSessionCommitmentsV1 {
            session_commitment_sha256: self.session_commitment_sha256.clone(),
            general_gesture_permission_commitment_sha256: self
                .launch
                .gesture_authorization
                .authorization_commitment_sha256
                .clone(),
            mode_authorization_commitment_sha256: self
                .launch
                .gesture_authorization
                .mode_authorization_commitment_sha256
                .clone(),
            pass_match_launch_commitment_sha256: self
                .launch
                .pass_match_launch
                .launch_authorization_commitment_sha256
                .clone(),
            match_gameplay_authorization_commitment_sha256: self
                .launch
                .pass_match_launch
                .gameplay_authorization_commitment_sha256
                .clone(),
            gesture_match_launch_commitment_sha256: self
                .launch
                .gesture_match_launch_commitment_sha256
                .clone(),
            gesture_evaluation_commitment_sha256: self
                .launch
                .gesture_authorization
                .gesture_evaluation_commitment_sha256
                .clone(),
            gesture_profile_admission_commitment_sha256: self
                .launch
                .gesture_authorization
                .gesture_profile_admission_commitment_sha256
                .clone(),
            event_kind: self.launch.pass_match_launch.authorization.event_kind,
            game_number: self.launch.pass_match_launch.authorization.game_number,
            valid_from_frame_sequence: self.launch.pass_match_launch.valid_from_frame_sequence,
            valid_through_frame_sequence: self
                .launch
                .pass_match_launch
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

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoSessionBoundCompetitiveDuelGestureCommitmentsV1 {
    pub binding_commitment_sha256: String,
    pub game_session_commitment_sha256: String,
    pub gesture_match_launch_commitment_sha256: String,
    pub gesture_evaluation_commitment_sha256: String,
    pub gesture_profile_admission_commitment_sha256: String,
    pub competitive_action_plan_commitment_sha256: String,
    pub gesture_plan_commitment_sha256: String,
    pub gesture_sequence_commitment_sha256: String,
    pub selected_action_family: MtgoDuelActionFamilyV1,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub source_frame_sequence: u64,
    pub gesture_stage_count: u16,
}

/// One source-stage gesture sequence joined to the move-only all-family game
/// session for the exact same mode, game, gameplay record, and frame lifetime.
/// It keeps the session and private target points opaque. It cannot prepare or
/// execute input.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoSessionBoundCompetitiveDuelGestureV1;
/// fn cannot_act(value: OpaqueMtgoSessionBoundCompetitiveDuelGestureV1) {
///     let _ = value.target_points_desktop_px();
///     let _ = value.send_input();
/// }
/// ```
pub struct OpaqueMtgoSessionBoundCompetitiveDuelGestureV1 {
    _session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    _sequence: OpaqueMtgoCompetitiveDuelGestureSequenceV1,
    commitments: MtgoSessionBoundCompetitiveDuelGestureCommitmentsV1,
}

impl OpaqueMtgoSessionBoundCompetitiveDuelGestureV1 {
    pub fn commitments_v1(&self) -> MtgoSessionBoundCompetitiveDuelGestureCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1 {
    pub preparation_binding_commitment_sha256: String,
    pub session_sequence_binding_commitment_sha256: String,
    pub game_session_commitment_sha256: String,
    pub gesture_match_launch_commitment_sha256: String,
    pub source_sequence_commitment_sha256: String,
    pub competitive_action_plan_commitment_sha256: String,
    pub gesture_plan_commitment_sha256: String,
    pub fresh_stage_binding_commitment_sha256: String,
    pub fresh_capture_commitment_sha256: String,
    pub fresh_perception_result_commitment_sha256: String,
    pub gesture_target_runtime_identity_commitment_sha256: String,
    pub gesture_target_request_commitment_sha256: String,
    pub primitive_commitment_sha256: String,
    pub selected_action_family: MtgoDuelActionFamilyV1,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub stage_index: u16,
    pub gesture_stage_count: u16,
    pub target_count: u16,
    pub fresh_frame_id: u64,
    pub fresh_frame_sequence: u64,
    pub fresh_captured_at_unix_millis: u128,
}

/// One session-bound source primitive rechecked against a distinct next-frame
/// opaque perception and the response from the exact profile-pinned gesture
/// target runtime. This wrapper has no input method and exposes no target
/// points.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1;
/// fn cannot_act(value: OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1) {
///     let _ = value.target_points_desktop_px();
///     let _ = value.send_input();
/// }
/// ```
pub struct OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1 {
    _session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    _prepared: ProbeOpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1,
    commitments: MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1,
}

impl OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1 {
    pub fn commitments_v1(&self) -> MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn target_runtime_attested_v1(&self) -> bool {
        true
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
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
    let permission_review_commitment_sha256 = correspondence.review_commitment_sha256().to_owned();
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(event_kind)
        .map_err(|error| format!("derive reviewed competitive mode scope: {error}"))?;
    let mode_authorization_commitment_sha256 =
        validate_competitive_duel_pass_authorization_v1(&scope, visible_account_alias, event_kind)?;
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

/// Computes the non-authorizing production-ratification candidate for the
/// complete reviewed duel gesture profile in one exact League or Challenge
/// mode. Both the private Daybreak reply and the admitted gesture evaluation
/// are retained by their opaque wrappers. No input, entry, or spending
/// authority is created.
pub fn review_competitive_duel_gesture_ratification_candidate_from_correspondence_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
    gesture_profile: &AdmittedMtgoDuelGestureProfileV1,
) -> Result<MtgoReviewedCompetitiveGestureRatificationCandidateV1, String> {
    let facts = CompetitiveDuelGestureProfileFactsV1 {
        evaluation_commitment_sha256: gesture_profile.evaluation_commitment_sha256(),
        admission_commitment_sha256: gesture_profile.admission_commitment_sha256(),
        runtime_binary_sha256: gesture_profile.gesture_target_runtime_binary_sha256(),
        assets_manifest_sha256: gesture_profile.gesture_target_assets_manifest_sha256(),
        supported_action_families: gesture_profile.supported_action_families(),
    };
    competitive_duel_gesture_ratification_candidate_from_parts_v1(
        correspondence,
        visible_account_alias,
        event_kind,
        &facts,
    )
}

/// Production constructor for one exact-account, exact-mode, reviewed gesture
/// permission. The compile-pinned root is empty until the correspondence and
/// measured all-family gesture corpus have both been reviewed. The returned
/// value remains a permission identity only and cannot send input.
pub fn ratify_competitive_duel_gesture_authorization_from_correspondence_v1(
    correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
    gesture_profile: AdmittedMtgoDuelGestureProfileV1,
) -> Result<RatifiedMtgoCompetitiveDuelGestureAuthorizationV1, String> {
    let candidate = review_competitive_duel_gesture_ratification_candidate_from_correspondence_v1(
        &correspondence,
        &visible_account_alias,
        event_kind,
        &gesture_profile,
    )?;
    if RATIFIED_COMPETITIVE_DUEL_GESTURE_AUTHORIZATION_FROM_REVIEW_COMMITMENT_V1
        != Some(candidate.ratification_commitment_sha256.as_str())
    {
        return Err(
            "the exact reviewed competitive gesture permission is not ratified in this build"
                .to_owned(),
        );
    }
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(event_kind)
        .map_err(|error| format!("derive reviewed competitive gesture mode scope: {error}"))?;
    Ok(RatifiedMtgoCompetitiveDuelGestureAuthorizationV1 {
        scope,
        _permission_correspondence: correspondence,
        _gesture_profile: gesture_profile,
        visible_account_alias,
        event_kind,
        mode_authorization_commitment_sha256: candidate.mode_authorization_commitment_sha256,
        permission_review_commitment_sha256: candidate.permission_review_commitment_sha256,
        gesture_evaluation_commitment_sha256: candidate.gesture_evaluation_commitment_sha256,
        gesture_profile_admission_commitment_sha256: candidate
            .gesture_profile_admission_commitment_sha256,
        authorization_commitment_sha256: candidate.ratification_commitment_sha256,
    })
}

/// Computes the exact production ratification candidate for one attended,
/// classifier-backed, visibly enabled League or Challenge entry review. The
/// output is non-authorizing telemetry. The private correspondence and the
/// opaque coordinate-bearing review remain retained by their callers.
pub fn review_competitive_entry_ratification_candidate_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    review: &CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4,
    visible_account_alias: &str,
) -> Result<MtgoReviewedCompetitiveEntryRatificationCandidateV1, String> {
    let source_identity = &review
        ._classifier_bound_review
        ._source_bound_review
        ._source_identity;
    competitive_entry_ratification_candidate_from_parts_v1(
        correspondence,
        visible_account_alias,
        source_identity.lifecycle_v1(),
        &source_identity.commitments_v1(),
        &review.entry_authorization,
        &review.commitments,
    )
}

/// Production path for separately ratifying one exact owner-reviewed League
/// or Challenge entry. The root is deliberately empty, so current builds
/// always reject. Even a future ratified value exposes no coordinate or input
/// method and must still pass immediate-recapture and postcondition gates.
pub fn ratify_competitive_entry_authorization_v1(
    correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    review: CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4,
    visible_account_alias: String,
) -> Result<RatifiedMtgoCompetitiveEntryAuthorizationV1, String> {
    ratify_competitive_entry_authorization_with_commitment_v1(
        correspondence,
        review,
        visible_account_alias,
        RATIFIED_COMPETITIVE_ENTRY_AUTHORIZATION_COMMITMENT_V1,
    )
}

/// Reacquires and reclassifies the exact owner-reviewed League or Challenge
/// entry immediately before its exact actuation boundary. This consumes the
/// separately ratified authorization and retains both it and the fresh opaque
/// frame. The result exposes no coordinates or general input capability and is
/// accepted only by the one-click, visible-confirmation-locked entry executor.
///
/// Current production builds cannot reach this function successfully because
/// both the navigation-profile and exact-entry ratification roots are empty.
pub fn prepare_ratified_competitive_entry_v1(
    authorization: RatifiedMtgoCompetitiveEntryAuthorizationV1,
    profile: &AdmittedMtgoCompetitiveNavigationProfileV1,
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    immediate_identity: MtgoCompetitiveNavigationFrameIdentityV1,
    capture_timeout_ms: u32,
    classifier_timeout_ms: u32,
) -> Result<OpaqueMtgoPreparedCompetitiveEntryV1, String> {
    let immediate_frame =
        capture_admitted_mtgo_competitive_navigation_frame_v1(profile, capture_timeout_ms)?;
    let immediate = classify_admitted_mtgo_competitive_navigation_frame_v1(
        immediate_frame,
        profile,
        runtime,
        immediate_identity,
        classifier_timeout_ms,
    )?;
    let recapture = {
        let review = &authorization._review;
        let source = &review
            ._classifier_bound_review
            ._source_bound_review
            ._source_identity;
        validate_classifier_backed_competitive_entry_immediate_recapture_v1(
            source,
            &review._control_rect_client_px,
            &review.commitments.entry_control_dry_run,
            &immediate,
        )?
    };
    let pointer_target = resolve_competitive_entry_pointer_target_v1(
        &immediate,
        &authorization._review._control_rect_client_px,
    )?;
    let commitments =
        competitive_entry_preparation_from_commitments_v1(&authorization.commitments, &recapture)?;
    Ok(OpaqueMtgoPreparedCompetitiveEntryV1 {
        _authorization: authorization,
        _immediate_frame: immediate,
        pointer_target,
        commitments,
    })
}

/// Emits exactly one left click for the exact freshly prepared entry and then
/// locks the shared process gate until its visible postcondition is confirmed.
/// Production builds cannot construct the prerequisite ratified entry or
/// admitted navigation profile because both trust roots remain empty.
pub fn execute_prepared_competitive_entry_v1(
    prepared: OpaqueMtgoPreparedCompetitiveEntryV1,
) -> Result<OpaqueMtgoPendingCompetitiveEntryV1, String> {
    reserve_input_gate_v3()?;
    let input_sent_at_unix_millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(error) => {
            release_unattempted_reservation_v3()?;
            return Err(format!("system clock is before epoch: {error}"));
        }
    };
    let prepared_commitments = prepared.commitments_v1();
    if validate_preinput_capture_freshness_v3(
        prepared_commitments
            .immediate_recapture
            .immediate_captured_at_unix_millis,
        input_sent_at_unix_millis,
    )
    .is_err()
    {
        release_unattempted_reservation_v3()?;
        return Err(
            "the immediate competitive entry capture is stale or from the future".to_owned(),
        );
    }
    halt_before_input_attempt_v3()?;
    let cursor_parked_outside_client = send_exactly_one_left_click_v3(&prepared)?;
    let input_receipt_sha256 = competitive_entry_input_receipt_v1(
        &prepared,
        input_sent_at_unix_millis,
        cursor_parked_outside_client,
    );
    set_pending_v3(&input_receipt_sha256)?;
    Ok(OpaqueMtgoPendingCompetitiveEntryV1 {
        prepared,
        commitments: MtgoCompetitiveEntryInputReceiptCommitmentsV1 {
            preparation_commitment_sha256: prepared_commitments.preparation_commitment_sha256,
            entry_ratification_commitment_sha256: prepared_commitments
                .entry_ratification_commitment_sha256,
            input_receipt_sha256,
            event_kind: prepared_commitments.event_kind,
            resource: prepared_commitments.resource,
            amount: prepared_commitments.amount,
            immediate_frame_id: prepared_commitments.immediate_frame_id,
            immediate_frame_sequence: prepared_commitments.immediate_frame_sequence,
            input_sent_at_unix_millis,
            cursor_parked_outside_client,
        },
    })
}

pub fn confirm_pending_competitive_entry_v1(
    pending: OpaqueMtgoPendingCompetitiveEntryV1,
    profile: &AdmittedMtgoCompetitiveNavigationProfileV1,
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoConfirmedCompetitiveEntryV1, String> {
    require_matching_pending_v3(&pending.commitments.input_receipt_sha256)?;
    let OpaqueMtgoPendingCompetitiveEntryV1 {
        prepared,
        commitments: input,
    } = pending;
    let OpaqueMtgoPreparedCompetitiveEntryV1 {
        _authorization: authorization,
        _immediate_frame: immediate_frame,
        pointer_target: _,
        commitments: prepared_commitments,
    } = prepared;
    let source = &authorization
        ._review
        ._classifier_bound_review
        ._source_bound_review
        ._source_identity;
    let source_identity_commitment_sha256 =
        source.commitments_v1().source_identity_commitment_sha256;
    let event_label_rect_client_px = source.event_label_rect_client_px_v1().clone();
    let control_rect_client_px = authorization._review._control_rect_client_px.clone();
    let visible_confirmation = match confirm_opaque_competitive_entry_postcondition_v1(
        immediate_frame,
        source_identity_commitment_sha256,
        event_label_rect_client_px,
        control_rect_client_px,
        prepared_commitments
            .immediate_recapture
            .event_label_region_sha256
            .clone(),
        prepared_commitments
            .immediate_recapture
            .visible_control_region_sha256
            .clone(),
        prepared_commitments.preparation_commitment_sha256.clone(),
        profile,
        runtime,
        input.input_sent_at_unix_millis,
        timeout_ms,
    ) {
        Ok(confirmation) => confirmation,
        Err(error) => {
            halt_gate_v3()?;
            return Err(format!(
                "competitive entry postcondition failed and the input gate is halted: {error}"
            ));
        }
    };
    let visible = visible_confirmation.commitments_v1();
    if visible.frame_transition.event_kind != prepared_commitments.event_kind
        || visible.frame_transition.event_identity_sha256
            != prepared_commitments
                .immediate_recapture
                .event_identity_sha256
        || visible.frame_transition.before_frame_id != prepared_commitments.immediate_frame_id
        || visible.frame_transition.before_frame_sequence
            != prepared_commitments.immediate_frame_sequence
        || visible.frame_transition.after_frame_sequence
            <= prepared_commitments.immediate_frame_sequence
        || visible.after_captured_at_unix_millis <= input.input_sent_at_unix_millis
    {
        halt_gate_v3()?;
        return Err(
            "competitive entry confirmation changed the exact event or frame and halted the input gate"
                .to_owned(),
        );
    }
    let confirmation_receipt_sha256 =
        competitive_entry_confirmation_receipt_v1(&input, &prepared_commitments, &visible);
    release_confirmed_pending_v3(&input.input_receipt_sha256)?;
    Ok(OpaqueMtgoConfirmedCompetitiveEntryV1 {
        _authorization: authorization,
        _visible_confirmation: visible_confirmation,
        commitments: MtgoConfirmedCompetitiveEntryCommitmentsV1 {
            input_receipt_sha256: input.input_receipt_sha256,
            preparation_commitment_sha256: prepared_commitments.preparation_commitment_sha256,
            entry_ratification_commitment_sha256: prepared_commitments
                .entry_ratification_commitment_sha256,
            visible_transition_commitment_sha256: visible
                .frame_transition
                .transition_commitment_sha256,
            visible_confirmation_commitment_sha256: visible.confirmation_commitment_sha256,
            confirmation_receipt_sha256,
            event_kind: prepared_commitments.event_kind,
            resource: prepared_commitments.resource,
            amount: prepared_commitments.amount,
            after_frame_id: visible.frame_transition.after_frame_id,
            after_frame_sequence: visible.frame_transition.after_frame_sequence,
            after_captured_at_unix_millis: visible.after_captured_at_unix_millis,
            postcondition_candidate_count: visible.postcondition_candidate_count,
        },
    })
}

/// Performs a terminal-attended review of one exact visible League or
/// Challenge entry and its exact existing-account resource terms. This is a
/// non-authorizing review boundary. It neither enters the event nor enables a
/// later input path.
pub fn review_competitive_entry_attended_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    visible_account_alias: &str,
    event_display_label: String,
) -> Result<CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1, String> {
    let request = build_attended_competitive_entry_review_request_v1(
        correspondence,
        &source,
        visible_account_alias,
        &event_display_label,
    )?;
    let (challenge_nonce, issued_at_unix_millis, supplied_phrase) =
        prompt_attended_competitive_entry_review_v1(
            &request,
            visible_account_alias,
            "checked visible lifecycle facts",
        )?;
    review_competitive_entry_from_attended_confirmation_v1(
        correspondence,
        source,
        visible_account_alias,
        event_display_label,
        challenge_nonce,
        issued_at_unix_millis,
        &supplied_phrase,
    )
}

/// Source-bound manual entry-review path. It consumes an identity that has already
/// rehashed every lifecycle fact against one retained opaque composed-desktop
/// navigation frame. The result remains non-actionable and cannot click Join
/// or spend resources.
pub fn review_competitive_entry_attended_v2(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source_identity: OpaqueMtgoCompetitiveEntryReviewIdentityV1,
    visible_account_alias: &str,
) -> Result<CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2, String> {
    let source_commitments = source_identity.commitments_v1();
    let source_description = if source_commitments
        .source_navigation_classification_result_commitment_sha256
        .is_some()
    {
        "exact profile-pinned classifier over opaque composed-desktop navigation pixels"
    } else {
        "opaque composed-desktop navigation pixels"
    };
    review_competitive_entry_attended_v2_with_source_description(
        correspondence,
        source_identity,
        visible_account_alias,
        source_description,
    )
}

fn review_competitive_entry_attended_v2_with_source_description(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source_identity: OpaqueMtgoCompetitiveEntryReviewIdentityV1,
    visible_account_alias: &str,
    source_description: &str,
) -> Result<CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2, String> {
    let source_commitments = source_identity.commitments_v1();
    let request = build_attended_competitive_entry_review_request_v1(
        correspondence,
        source_identity.lifecycle_v1(),
        visible_account_alias,
        source_identity.event_display_label_v1(),
    )?;
    if request.source_lifecycle_snapshot_commitment_sha256
        != source_commitments.source_lifecycle_snapshot_commitment_sha256
        || request.event_kind != source_commitments.event_kind
        || request.frame_id != source_commitments.frame_id
        || request.frame_sequence != source_commitments.frame_sequence
        || request.entry_terms.resource != source_commitments.resource
        || request.entry_terms.amount != source_commitments.amount
    {
        return Err(
            "source-bound entry review identity changed before owner confirmation".to_owned(),
        );
    }
    let (challenge_nonce, issued_at_unix_millis, supplied_phrase) =
        prompt_attended_competitive_entry_review_v1(
            &request,
            visible_account_alias,
            source_description,
        )?;
    let parts = make_attended_competitive_entry_review_parts_v1(
        correspondence,
        source_identity.lifecycle_v1(),
        visible_account_alias,
        source_identity.event_display_label_v1(),
        challenge_nonce,
        issued_at_unix_millis,
        &supplied_phrase,
    )?;
    Ok(CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2 {
        _source_identity: source_identity,
        entry_authorization: parts.entry_authorization,
        commitments: MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2 {
            source_identity_commitment_sha256: source_commitments.source_identity_commitment_sha256,
            source_capture_commitment_sha256: source_commitments.source_capture_commitment_sha256,
            source_navigation_classification_result_commitment_sha256: source_commitments
                .source_navigation_classification_result_commitment_sha256,
            attended_review: parts.commitments,
        },
    })
}

/// Preferred classifier-bound entry-review path. The consumed source must
/// retain the exact profile-pinned classifier result that produced its visible
/// lifecycle interpretation. Success still cannot click Join, spend resources,
/// or enable input.
pub fn review_competitive_entry_attended_v3(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source_identity: OpaqueMtgoCompetitiveEntryReviewIdentityV1,
    visible_account_alias: &str,
) -> Result<CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3, String> {
    review_competitive_entry_attended_v3_with_source_description(
        correspondence,
        source_identity,
        visible_account_alias,
        "exact profile-pinned classifier over opaque composed-desktop navigation pixels",
    )
}

fn review_competitive_entry_attended_v3_with_source_description(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source_identity: OpaqueMtgoCompetitiveEntryReviewIdentityV1,
    visible_account_alias: &str,
    source_description: &str,
) -> Result<CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3, String> {
    let source_commitments = source_identity.commitments_v1();
    let classifier_commitment =
        require_classifier_bound_competitive_entry_source_v3(&source_commitments)?.to_owned();
    let source_bound_review = review_competitive_entry_attended_v2_with_source_description(
        correspondence,
        source_identity,
        visible_account_alias,
        source_description,
    )?;
    let source_bound_commitments = source_bound_review.commitments_v2();
    if source_bound_commitments
        .source_navigation_classification_result_commitment_sha256
        .as_deref()
        != Some(classifier_commitment.as_str())
    {
        return Err(
            "classifier-bound entry review lineage changed during owner confirmation".to_owned(),
        );
    }
    let classifier_bound_review_commitment_sha256 =
        classifier_bound_competitive_entry_review_commitment_v3(
            &source_bound_commitments,
            &classifier_commitment,
        );
    let entry_authorization = source_bound_review.entry_authorization_record_v2();
    Ok(
        CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3 {
            _source_bound_review: source_bound_review,
            entry_authorization,
            commitments: MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3 {
                source_navigation_classification_result_commitment_sha256: classifier_commitment,
                classifier_bound_review_commitment_sha256,
                source_bound_review: source_bound_commitments,
            },
        },
    )
}

/// Preferred owner-attended competitive entry dry run. It consumes a visible,
/// explicitly enabled Confirm Entry control bound to the same exact opaque
/// classifier-backed frame as the event and terms review. Success records the
/// review but cannot click Join, spend resources, or enable input.
pub fn review_competitive_entry_attended_v4(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    dry_run: OpaqueMtgoCompetitiveEntryControlDryRunV1,
    visible_account_alias: &str,
) -> Result<CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4, String> {
    let MtgoCompetitiveEntryControlDryRunPartsV1 {
        source_identity,
        visible_control_label,
        control_rect_client_px,
        commitments: entry_control_dry_run,
    } = dry_run.into_parts_v1();
    let source_description = format!(
        "exact profile-pinned classifier plus human-reviewed visibly enabled control {visible_control_label:?} over opaque composed-desktop navigation pixels"
    );
    let classifier_bound_review = review_competitive_entry_attended_v3_with_source_description(
        correspondence,
        source_identity,
        visible_account_alias,
        &source_description,
    )?;
    let classifier_bound_commitments = classifier_bound_review.commitments_v3();
    let control_bound_review_commitment_sha256 =
        bind_control_bound_competitive_entry_review_commitment_v4(
            &classifier_bound_commitments,
            &entry_control_dry_run,
        )?;
    let entry_authorization = classifier_bound_review.entry_authorization_record_v3();
    Ok(CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4 {
        _classifier_bound_review: classifier_bound_review,
        _visible_control_label: visible_control_label,
        _control_rect_client_px: control_rect_client_px,
        entry_authorization,
        commitments: MtgoControlBoundCompetitiveEntryReviewCommitmentsV4 {
            control_bound_review_commitment_sha256,
            classifier_bound_review: classifier_bound_commitments,
            entry_control_dry_run,
        },
    })
}

/// Pairs the exact owner-attended, control-bound entry review with one strictly
/// newer classifier-backed entered-waiting frame. This records the visible
/// postcondition shape only. It does not create or execute ConfirmEntry.
pub fn bind_competitive_entry_postcondition_dry_run_v1(
    source_review: CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4,
    entered_waiting_frame: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
) -> Result<CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1, String> {
    let frame_transition = validate_classifier_backed_competitive_entry_frame_transition_v1(
        &source_review
            ._classifier_bound_review
            ._source_bound_review
            ._source_identity,
        &entered_waiting_frame,
    )?;
    let entry_event_identity_sha256 = source_review
        .entry_authorization
        .event_identity_sha256
        .clone();
    let review = source_review.commitments_v4();
    let source_bound = &review.classifier_bound_review.source_bound_review;
    let attended = &source_bound.attended_review;
    if review
        .entry_control_dry_run
        .source_identity_commitment_sha256
        != frame_transition.source_identity_commitment_sha256
        || review
            .entry_control_dry_run
            .source_capture_commitment_sha256
            != frame_transition.before_capture_commitment_sha256
        || review
            .entry_control_dry_run
            .source_navigation_classification_result_commitment_sha256
            != frame_transition.before_classification_result_commitment_sha256
        || review
            .entry_control_dry_run
            .source_lifecycle_snapshot_commitment_sha256
            != frame_transition.before_lifecycle_snapshot_commitment_sha256
        || review
            .classifier_bound_review
            .source_navigation_classification_result_commitment_sha256
            != frame_transition.before_classification_result_commitment_sha256
        || source_bound.source_identity_commitment_sha256
            != frame_transition.source_identity_commitment_sha256
        || source_bound.source_capture_commitment_sha256
            != frame_transition.before_capture_commitment_sha256
        || attended.source_lifecycle_snapshot_commitment_sha256
            != frame_transition.before_lifecycle_snapshot_commitment_sha256
        || attended.event_kind != frame_transition.event_kind
        || attended.frame_id != frame_transition.before_frame_id
        || attended.frame_sequence != frame_transition.before_frame_sequence
        || entry_event_identity_sha256 != frame_transition.event_identity_sha256
        || attended.resource != review.entry_control_dry_run.resource
        || attended.amount != review.entry_control_dry_run.amount
    {
        return Err(
            "competitive entry postcondition dry run changed the exact review, control, classifier, frame, event, or terms"
                .to_owned(),
        );
    }
    for digest in [
        review.control_bound_review_commitment_sha256.as_str(),
        attended.entry_authorization_sha256.as_str(),
        frame_transition.transition_commitment_sha256.as_str(),
    ] {
        if !is_sha256_v2(digest) {
            return Err(
                "competitive entry postcondition dry run contains an invalid commitment".to_owned(),
            );
        }
    }
    let event_kind: &[u8] = match attended.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let resource: &[u8] = match attended.resource {
        MtgoCompetitiveEntryResourceV1::NoCost => b"no_cost",
        MtgoCompetitiveEntryResourceV1::ExistingPlayPoints => b"existing_play_points",
        MtgoCompetitiveEntryResourceV1::ExistingEventTickets => b"existing_event_tickets",
        MtgoCompetitiveEntryResourceV1::ExistingEventToken => b"existing_event_token",
    };
    let postcondition_dry_run_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_ENTRY_POSTCONDITION_DRY_RUN_DOMAIN_V1,
        &[
            review.control_bound_review_commitment_sha256.as_bytes(),
            attended.entry_authorization_sha256.as_bytes(),
            frame_transition.transition_commitment_sha256.as_bytes(),
            frame_transition
                .before_classification_result_commitment_sha256
                .as_bytes(),
            frame_transition
                .after_classification_result_commitment_sha256
                .as_bytes(),
            event_kind,
            entry_event_identity_sha256.as_bytes(),
            resource,
            attended.amount.to_be_bytes().as_slice(),
            b"visible_postcondition_calibration_pair_no_causality_no_join_no_spending_no_input",
        ],
    );
    let commitments = MtgoCompetitiveEntryPostconditionDryRunCommitmentsV1 {
        control_bound_review_commitment_sha256: review.control_bound_review_commitment_sha256,
        entry_authorization_sha256: attended.entry_authorization_sha256.clone(),
        frame_transition,
        postcondition_dry_run_commitment_sha256,
        event_kind: attended.event_kind,
        resource: attended.resource,
        amount: attended.amount,
    };
    Ok(CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1 {
        _source_review: source_review,
        _entered_waiting_frame: entered_waiting_frame,
        commitments,
    })
}

fn require_classifier_bound_competitive_entry_source_v3(
    source: &MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1,
) -> Result<&str, String> {
    let classifier_commitment = source
        .source_navigation_classification_result_commitment_sha256
        .as_deref()
        .ok_or(
            "classifier-bound entry review requires an exact retained navigation classifier result",
        )?;
    if !is_sha256_v2(classifier_commitment) {
        return Err(
            "classifier-bound entry review contains an invalid classifier commitment".to_owned(),
        );
    }
    Ok(classifier_commitment)
}

fn classifier_bound_competitive_entry_review_commitment_v3(
    source_bound: &MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2,
    classifier_commitment_sha256: &str,
) -> String {
    let event_kind: &[u8] = match source_bound.attended_review.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let resource: &[u8] = match source_bound.attended_review.resource {
        MtgoCompetitiveEntryResourceV1::NoCost => b"no_cost",
        MtgoCompetitiveEntryResourceV1::ExistingPlayPoints => b"existing_play_points",
        MtgoCompetitiveEntryResourceV1::ExistingEventTickets => b"existing_event_tickets",
        MtgoCompetitiveEntryResourceV1::ExistingEventToken => b"existing_event_token",
    };
    hash_parts_v2(
        CLASSIFIER_BOUND_COMPETITIVE_ENTRY_REVIEW_DOMAIN_V3,
        &[
            classifier_commitment_sha256.as_bytes(),
            source_bound.source_identity_commitment_sha256.as_bytes(),
            source_bound.source_capture_commitment_sha256.as_bytes(),
            source_bound
                .attended_review
                .source_lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            source_bound
                .attended_review
                .permission_review_commitment_sha256
                .as_bytes(),
            source_bound
                .attended_review
                .mode_authorization_commitment_sha256
                .as_bytes(),
            source_bound
                .attended_review
                .request_commitment_sha256
                .as_bytes(),
            source_bound
                .attended_review
                .entry_authorization_sha256
                .as_bytes(),
            source_bound
                .attended_review
                .owner_review_receipt_sha256
                .as_bytes(),
            event_kind,
            source_bound
                .attended_review
                .frame_id
                .to_be_bytes()
                .as_slice(),
            source_bound
                .attended_review
                .frame_sequence
                .to_be_bytes()
                .as_slice(),
            resource,
            source_bound.attended_review.amount.to_be_bytes().as_slice(),
            b"classifier_bound_owner_review_no_entry_no_spending_no_input",
        ],
    )
}

fn bind_control_bound_competitive_entry_review_commitment_v4(
    classifier_bound: &MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3,
    dry_run: &MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1,
) -> Result<String, String> {
    let source_bound = &classifier_bound.source_bound_review;
    let attended = &source_bound.attended_review;
    if !dry_run.visibly_enabled_confirmed
        || classifier_bound.source_navigation_classification_result_commitment_sha256
            != dry_run.source_navigation_classification_result_commitment_sha256
        || source_bound
            .source_navigation_classification_result_commitment_sha256
            .as_deref()
            != Some(
                dry_run
                    .source_navigation_classification_result_commitment_sha256
                    .as_str(),
            )
        || source_bound.source_identity_commitment_sha256
            != dry_run.source_identity_commitment_sha256
        || source_bound.source_capture_commitment_sha256 != dry_run.source_capture_commitment_sha256
        || attended.source_lifecycle_snapshot_commitment_sha256
            != dry_run.source_lifecycle_snapshot_commitment_sha256
        || attended.event_kind != dry_run.event_kind
        || attended.frame_id != dry_run.frame_id
        || attended.frame_sequence != dry_run.frame_sequence
        || attended.resource != dry_run.resource
        || attended.amount != dry_run.amount
    {
        return Err(
            "control-bound competitive entry review does not match its exact classifier, frame, lifecycle, event, and terms"
                .to_owned(),
        );
    }
    for commitment in [
        classifier_bound
            .classifier_bound_review_commitment_sha256
            .as_str(),
        dry_run.dry_run_commitment_sha256.as_str(),
        dry_run.visible_control_label_sha256.as_str(),
        dry_run.visible_control_region_sha256.as_str(),
    ] {
        if !is_sha256_v2(commitment) {
            return Err(
                "control-bound competitive entry review contains an invalid commitment".to_owned(),
            );
        }
    }
    Ok(hash_parts_v2(
        CONTROL_BOUND_COMPETITIVE_ENTRY_REVIEW_DOMAIN_V4,
        &[
            classifier_bound
                .classifier_bound_review_commitment_sha256
                .as_bytes(),
            dry_run.dry_run_commitment_sha256.as_bytes(),
            dry_run.source_identity_commitment_sha256.as_bytes(),
            dry_run.source_capture_commitment_sha256.as_bytes(),
            dry_run
                .source_lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            dry_run
                .source_navigation_classification_result_commitment_sha256
                .as_bytes(),
            dry_run.visible_control_label_sha256.as_bytes(),
            dry_run.visible_control_region_sha256.as_bytes(),
            &[u8::from(dry_run.visibly_enabled_confirmed)],
            attended.owner_review_receipt_sha256.as_bytes(),
            attended.entry_authorization_sha256.as_bytes(),
            b"owner_attended_control_bound_entry_dry_run_no_join_no_spending_no_input",
        ],
    ))
}

fn prompt_attended_competitive_entry_review_v1(
    request: &MtgoAttendedCompetitiveEntryReviewRequestV1,
    visible_account_alias: &str,
    source_description: &str,
) -> Result<([u8; 8], u128, String), String> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    if !stdin.is_terminal() || !stdout.is_terminal() {
        return Err(
            "attended competitive entry review requires an interactive terminal".to_owned(),
        );
    }
    let mut challenge_nonce = [0_u8; 8];
    unsafe {
        BCryptGenRandom(None, &mut challenge_nonce, BCRYPT_USE_SYSTEM_PREFERRED_RNG)
            .ok()
            .map_err(|error| format!("generate attended entry review challenge: {error}"))?;
    }
    let issued_at_unix_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before epoch: {error}"))?
        .as_millis();
    let expected_phrase = attended_competitive_entry_review_confirmation_phrase_v1(
        request.event_kind,
        request.entry_terms.resource,
        request.entry_terms.amount,
        &challenge_nonce,
    );
    let event_prefix = &request.event_identity_sha256[..12];
    let terms_prefix = &request.entry_terms.terms_sha256[..12];
    writeln!(stdout, "MTGO attended competitive entry review")
        .map_err(|error| format!("write attended entry review prompt: {error}"))?;
    writeln!(stdout, "Account: {visible_account_alias}")
        .map_err(|error| format!("write attended entry review account: {error}"))?;
    writeln!(stdout, "Visible source: {source_description}")
        .map_err(|error| format!("write attended entry review source: {error}"))?;
    writeln!(
        stdout,
        "Mode: {}; event: {}; resource: {}; exact amount: {}; event id: {event_prefix}; terms id: {terms_prefix}",
        competitive_event_kind_label_v4(request.event_kind),
        request.event_display_label,
        competitive_entry_resource_label_v1(request.entry_terms.resource),
        request.entry_terms.amount,
    )
    .map_err(|error| format!("write attended entry review terms: {error}"))?;
    writeln!(
        stdout,
        "This records your review only. It does not enter the event, spend resources, or enable input."
    )
    .map_err(|error| format!("write attended entry review scope: {error}"))?;
    writeln!(stdout, "Type exactly: {expected_phrase}")
        .map_err(|error| format!("write attended entry review challenge: {error}"))?;
    stdout
        .flush()
        .map_err(|error| format!("flush attended entry review prompt: {error}"))?;
    let mut supplied_phrase = String::new();
    stdin
        .read_line(&mut supplied_phrase)
        .map_err(|error| format!("read attended entry review confirmation: {error}"))?;
    Ok((
        challenge_nonce,
        issued_at_unix_millis,
        supplied_phrase.trim_end_matches(['\r', '\n']).to_owned(),
    ))
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
        entry_authorization_sha256: visible_identity.entry_authorization_sha256_v1().to_owned(),
        observed_frame_sequence: source.frame_sequence,
        source_capture_commitment_sha256: source.source_capture_commitment_sha256,
        source_perception_result_commitment_sha256: source.perception_result_commitment_sha256,
        source_lifecycle_snapshot_commitment_sha256: source.lifecycle_snapshot_commitment_sha256,
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

/// Extends one separately attended priority-Pass launch to the exact admitted
/// eleven-family gesture profile. The owner must confirm the broader scope in
/// an interactive terminal. This consumes both authorities and creates no
/// input, event-entry, or spending capability.
pub fn ratify_competitive_gesture_match_launch_attended_v1(
    gesture_authorization: RatifiedMtgoCompetitiveDuelGestureAuthorizationV1,
    pass_match_launch: RatifiedMtgoCompetitiveMatchLaunchV1,
) -> Result<RatifiedMtgoCompetitiveGestureMatchLaunchV1, String> {
    let candidate = validate_competitive_gesture_match_launch_authorities_v1(
        &gesture_authorization,
        &pass_match_launch,
    )?;
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    if !stdin.is_terminal() || !stdout.is_terminal() {
        return Err(
            "attended competitive gesture match launch requires an interactive terminal".to_owned(),
        );
    }
    let mut challenge_nonce = [0_u8; 8];
    unsafe {
        BCryptGenRandom(None, &mut challenge_nonce, BCRYPT_USE_SYSTEM_PREFERRED_RNG)
            .ok()
            .map_err(|error| format!("generate attended gesture launch challenge: {error}"))?;
    }
    let issued_at_unix_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before epoch: {error}"))?
        .as_millis();
    let expected_phrase = attended_competitive_gesture_match_launch_confirmation_phrase_v1(
        candidate.event_kind,
        pass_match_launch.authorization.game_number,
        &challenge_nonce,
    );
    writeln!(stdout, "MTGO attended all-family match launch extension")
        .map_err(|error| format!("write attended gesture launch prompt: {error}"))?;
    writeln!(
        stdout,
        "Account: {}",
        gesture_authorization.visible_account_alias
    )
    .map_err(|error| format!("write attended gesture launch account: {error}"))?;
    writeln!(
        stdout,
        "Mode: {}; game: {}; exact match launch: {}; gesture evaluation: {}; gesture profile: {}",
        competitive_event_kind_label_v4(candidate.event_kind),
        pass_match_launch.authorization.game_number,
        &pass_match_launch.launch_authorization_commitment_sha256[..12],
        &candidate.gesture_evaluation_commitment_sha256[..12],
        &candidate.gesture_profile_admission_commitment_sha256[..12]
    )
    .map_err(|error| format!("write attended gesture launch identity: {error}"))?;
    writeln!(
        stdout,
        "This extends the exact already-entered game from priority Pass to the reviewed eleven-family gesture profile. It cannot enter an event or spend resources."
    )
    .map_err(|error| format!("write attended gesture launch scope: {error}"))?;
    writeln!(stdout, "Type exactly: {expected_phrase}")
        .map_err(|error| format!("write attended gesture launch challenge: {error}"))?;
    stdout
        .flush()
        .map_err(|error| format!("flush attended gesture launch prompt: {error}"))?;
    let mut supplied_phrase = String::new();
    stdin
        .read_line(&mut supplied_phrase)
        .map_err(|error| format!("read attended gesture launch confirmation: {error}"))?;
    ratify_competitive_gesture_match_launch_from_confirmation_v1(
        gesture_authorization,
        pass_match_launch,
        candidate,
        challenge_nonce,
        issued_at_unix_millis,
        supplied_phrase.trim_end_matches(['\r', '\n']),
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

/// Begins a move-only all-family session for one exact attended League or
/// Challenge game. The session is a lineage boundary only. It cannot prepare
/// or execute a gesture in this tranche.
pub fn begin_competitive_gesture_game_session_v1(
    launch: RatifiedMtgoCompetitiveGestureMatchLaunchV1,
) -> Result<OpaqueMtgoCompetitiveGestureGameSessionV1, String> {
    let candidate = validate_competitive_gesture_match_launch_authorities_v1(
        &launch.gesture_authorization,
        &launch.pass_match_launch,
    )?;
    if !is_sha256_v2(&launch.gesture_match_launch_commitment_sha256) {
        return Err("competitive gesture match launch commitment is invalid".to_owned());
    }
    let valid_from_frame_sequence = launch.pass_match_launch.valid_from_frame_sequence;
    let last_confirmed_frame_sequence = valid_from_frame_sequence
        .checked_sub(1)
        .ok_or("competitive gesture game session has an invalid starting frame")?;
    let session_commitment_sha256 = initial_competitive_gesture_game_session_commitment_v1(
        &candidate,
        &launch.pass_match_launch,
        &launch.gesture_match_launch_commitment_sha256,
    );
    Ok(OpaqueMtgoCompetitiveGestureGameSessionV1 {
        launch,
        session_commitment_sha256,
        last_confirmed_frame_sequence,
        confirmed_action_count: 0,
    })
}

/// Joins the source stage of one exact gesture sequence to the matching
/// all-family game session. This consumes both move-only lineages and creates
/// no preparation, executor, or coordinate access.
pub fn bind_competitive_duel_gesture_sequence_session_v1(
    sequence: OpaqueMtgoCompetitiveDuelGestureSequenceV1,
    session: OpaqueMtgoCompetitiveGestureGameSessionV1,
) -> Result<OpaqueMtgoSessionBoundCompetitiveDuelGestureV1, String> {
    let sequence_commitments = sequence.commitments_v1();
    let session_commitments = session.commitments_v1();
    let commitments = competitive_duel_gesture_sequence_session_binding_from_parts_v1(
        &sequence_commitments,
        &session_commitments,
    )?;
    Ok(OpaqueMtgoSessionBoundCompetitiveDuelGestureV1 {
        _session: session,
        _sequence: sequence,
        commitments,
    })
}

/// Rechecks the source primitive of one exact session-bound gesture on a
/// distinct next-frame perception. Only the exact profile-pinned gesture
/// target runtime may provide the complete target set. The request and runtime
/// identities are retained in the result, which cannot send input and exposes
/// no target coordinates.
pub fn prepare_session_bound_competitive_duel_gesture_source_stage_v1(
    bound: OpaqueMtgoSessionBoundCompetitiveDuelGestureV1,
    fresh_perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    runtime: &OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1, String> {
    let OpaqueMtgoSessionBoundCompetitiveDuelGestureV1 {
        _session: session,
        _sequence: sequence,
        commitments: bound_commitments,
    } = bound;
    let session_commitments = session.commitments_v1();
    let profile = &session.launch.gesture_authorization._gesture_profile;
    if bound_commitments.gesture_evaluation_commitment_sha256
        != profile.evaluation_commitment_sha256()
        || bound_commitments.gesture_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
    {
        return Err("session-bound gesture differs from its retained profile".to_owned());
    }
    let prepared = prepare_opaque_competitive_duel_gesture_source_stage_from_pinned_runtime_v1(
        sequence,
        fresh_perception,
        profile,
        runtime,
        timeout_ms,
    )?;
    let commitments = competitive_duel_gesture_source_preparation_from_parts_v1(
        &bound_commitments,
        &session_commitments,
        &prepared.commitments,
    )?;
    Ok(OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1 {
        _session: session,
        _prepared: prepared,
        commitments,
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
    let candidate = review_competitive_duel_pass_ratification_candidate_from_correspondence_v2(
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

fn build_attended_competitive_entry_review_request_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    visible_account_alias: &str,
    event_display_label: &str,
) -> Result<MtgoAttendedCompetitiveEntryReviewRequestV1, String> {
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(source.event_kind())
        .map_err(|error| format!("derive reviewed competitive entry scope: {error}"))?;
    let mode_authorization_commitment_sha256 = validate_competitive_duel_pass_authorization_v1(
        &scope,
        visible_account_alias,
        source.event_kind(),
    )?;
    let request = MtgoAttendedCompetitiveEntryReviewRequestV1 {
        schema_version: MTGO_ATTENDED_COMPETITIVE_ENTRY_REVIEW_SCHEMA_V1,
        event_kind: source.event_kind(),
        event_display_label: event_display_label.to_owned(),
        source_lifecycle_snapshot_commitment_sha256: source.snapshot_commitment_sha256().to_owned(),
        frame_id: source.frame_id_v1(),
        frame_sequence: source.frame_sequence(),
        event_identity_sha256: source
            .event_identity_sha256_v1()
            .ok_or("entry-review snapshot has no exact event identity")?
            .to_owned(),
        entry_terms: source
            .entry_terms_v1()
            .ok_or("entry-review snapshot has no exact visible entry terms")?
            .clone(),
        account_alias_sha256: scope.account_alias_sha256.clone(),
        correspondence_sha256: scope.written_permission_sha256.clone(),
        permission_review_commitment_sha256: correspondence.review_commitment_sha256().to_owned(),
        mode_authorization_commitment_sha256,
    };
    validate_attended_competitive_entry_review_request_v1(
        correspondence,
        source,
        visible_account_alias,
        &request,
    )?;
    Ok(request)
}

fn review_competitive_entry_from_attended_confirmation_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    visible_account_alias: &str,
    event_display_label: String,
    challenge_nonce: [u8; 8],
    issued_at_unix_millis: u128,
    supplied_phrase: &str,
) -> Result<CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1, String> {
    let parts = make_attended_competitive_entry_review_parts_v1(
        correspondence,
        &source,
        visible_account_alias,
        &event_display_label,
        challenge_nonce,
        issued_at_unix_millis,
        supplied_phrase,
    )?;
    Ok(CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1 {
        _source: source,
        entry_authorization: parts.entry_authorization,
        commitments: parts.commitments,
    })
}

fn make_attended_competitive_entry_review_parts_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    visible_account_alias: &str,
    event_display_label: &str,
    challenge_nonce: [u8; 8],
    issued_at_unix_millis: u128,
    supplied_phrase: &str,
) -> Result<MtgoAttendedCompetitiveEntryReviewPartsV1, String> {
    let request = build_attended_competitive_entry_review_request_v1(
        correspondence,
        source,
        visible_account_alias,
        event_display_label,
    )?;
    if issued_at_unix_millis == 0 || challenge_nonce.iter().all(|value| *value == 0) {
        return Err("attended competitive entry review challenge is invalid".to_owned());
    }
    let expected_phrase = attended_competitive_entry_review_confirmation_phrase_v1(
        request.event_kind,
        request.entry_terms.resource,
        request.entry_terms.amount,
        &challenge_nonce,
    );
    if supplied_phrase != expected_phrase {
        return Err("attended competitive entry review challenge did not match".to_owned());
    }
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(request.event_kind)
        .map_err(|error| format!("derive reviewed competitive entry scope: {error}"))?;
    let entry_authorization = MtgoCompetitiveEntryAuthorizationV1 {
        schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        account_alias_sha256: request.account_alias_sha256.clone(),
        written_permission_sha256: request.correspondence_sha256.clone(),
        event_kind: request.event_kind,
        event_identity_sha256: request.event_identity_sha256.clone(),
        entry_terms: request.entry_terms.clone(),
        exact_entry_authorized: true,
        existing_account_resources_only: true,
    };
    let intent = make_offline_competitive_lifecycle_intent_v1(
        source,
        MtgoCompetitiveLifecycleActionV1::ConfirmEntry,
        &scope,
        Some(&entry_authorization),
    )
    .map_err(|error| format!("validate exact offline competitive entry review: {error}"))?;
    let entry_authorization_sha256 = intent
        .entry_authorization_sha256
        .ok_or("validated competitive entry intent omitted its authorization commitment")?;
    let request_json = serde_json::to_vec(&request)
        .map_err(|error| format!("serialize attended competitive entry review: {error}"))?;
    let request_commitment_sha256 = hash_parts_v2(
        ATTENDED_COMPETITIVE_ENTRY_REVIEW_REQUEST_DOMAIN_V1,
        &[
            request_json.as_slice(),
            visible_account_alias.as_bytes(),
            b"owner_readable_label_checked_visible_terms_review_only",
        ],
    );
    let owner_review_receipt_sha256 = hash_parts_v2(
        ATTENDED_COMPETITIVE_ENTRY_REVIEW_RECEIPT_DOMAIN_V1,
        &[
            request_commitment_sha256.as_bytes(),
            entry_authorization_sha256.as_bytes(),
            challenge_nonce.as_slice(),
            issued_at_unix_millis.to_be_bytes().as_slice(),
            supplied_phrase.as_bytes(),
            b"interactive_terminal_owner_review_no_entry_no_spending_no_input",
        ],
    );
    let commitments = MtgoAttendedCompetitiveEntryReviewCommitmentsV1 {
        source_lifecycle_snapshot_commitment_sha256: request
            .source_lifecycle_snapshot_commitment_sha256,
        permission_review_commitment_sha256: request.permission_review_commitment_sha256,
        mode_authorization_commitment_sha256: request.mode_authorization_commitment_sha256,
        request_commitment_sha256,
        entry_authorization_sha256,
        owner_review_receipt_sha256,
        event_kind: request.event_kind,
        frame_id: request.frame_id,
        frame_sequence: request.frame_sequence,
        resource: request.entry_terms.resource,
        amount: request.entry_terms.amount,
    };
    Ok(MtgoAttendedCompetitiveEntryReviewPartsV1 {
        entry_authorization,
        commitments,
    })
}

fn validate_attended_competitive_entry_review_request_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    visible_account_alias: &str,
    request: &MtgoAttendedCompetitiveEntryReviewRequestV1,
) -> Result<(), String> {
    if request.schema_version != MTGO_ATTENDED_COMPETITIVE_ENTRY_REVIEW_SCHEMA_V1
        || source.phase() != MtgoCompetitiveLifecyclePhaseV1::EntryReview
    {
        return Err(
            "attended competitive entry review requires the exact entry-review phase".to_owned(),
        );
    }
    validate_attended_launch_display_label_v4(
        &request.event_display_label,
        160,
        "entry event display label",
    )?;
    let expected_mode_word = match request.event_kind {
        MtgoCompetitiveEventKindV1::League => "league",
        MtgoCompetitiveEventKindV1::Challenge => "challenge",
    };
    if !request
        .event_display_label
        .to_ascii_lowercase()
        .contains(expected_mode_word)
    {
        return Err("attended entry event label does not identify the selected mode".to_owned());
    }
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(request.event_kind)
        .map_err(|error| format!("derive reviewed competitive entry scope: {error}"))?;
    let expected_mode_commitment = validate_competitive_duel_pass_authorization_v1(
        &scope,
        visible_account_alias,
        request.event_kind,
    )?;
    let source_event_identity = source
        .event_identity_sha256_v1()
        .ok_or("entry-review snapshot has no exact event identity")?;
    let source_entry_terms = source
        .entry_terms_v1()
        .ok_or("entry-review snapshot has no exact visible entry terms")?;
    if request.event_kind != source.event_kind()
        || request.source_lifecycle_snapshot_commitment_sha256
            != source.snapshot_commitment_sha256()
        || request.frame_id != source.frame_id_v1()
        || request.frame_sequence != source.frame_sequence()
        || request.event_identity_sha256 != source_event_identity
        || &request.entry_terms != source_entry_terms
        || request.account_alias_sha256 != scope.account_alias_sha256
        || request.correspondence_sha256 != correspondence.correspondence_sha256()
        || request.permission_review_commitment_sha256 != correspondence.review_commitment_sha256()
        || request.mode_authorization_commitment_sha256 != expected_mode_commitment
    {
        return Err(
            "attended competitive entry review does not match the exact permission, mode, frame, event, and visible terms"
                .to_owned(),
        );
    }
    for value in [
        request.source_lifecycle_snapshot_commitment_sha256.as_str(),
        request.event_identity_sha256.as_str(),
        request.entry_terms.terms_sha256.as_str(),
        request.account_alias_sha256.as_str(),
        request.correspondence_sha256.as_str(),
        request.permission_review_commitment_sha256.as_str(),
        request.mode_authorization_commitment_sha256.as_str(),
    ] {
        if !is_sha256_v2(value) {
            return Err(
                "attended competitive entry review contains an invalid commitment".to_owned(),
            );
        }
    }
    Ok(())
}

fn attended_competitive_entry_review_confirmation_phrase_v1(
    event_kind: MtgoCompetitiveEventKindV1,
    resource: MtgoCompetitiveEntryResourceV1,
    amount: u32,
    challenge_nonce: &[u8; 8],
) -> String {
    let nonce = challenge_nonce
        .iter()
        .map(|value| format!("{value:02X}"))
        .collect::<String>();
    format!(
        "AUTHORIZE MTGO {} ENTRY USING {amount} {} {nonce}",
        competitive_event_kind_label_v4(event_kind).to_ascii_uppercase(),
        competitive_entry_resource_label_v1(resource).to_ascii_uppercase(),
    )
}

fn competitive_entry_resource_label_v1(resource: MtgoCompetitiveEntryResourceV1) -> &'static str {
    match resource {
        MtgoCompetitiveEntryResourceV1::NoCost => "no cost",
        MtgoCompetitiveEntryResourceV1::ExistingEventToken => "existing event token",
        MtgoCompetitiveEntryResourceV1::ExistingPlayPoints => "existing play points",
        MtgoCompetitiveEntryResourceV1::ExistingEventTickets => "existing event tickets",
    }
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

fn validate_competitive_gesture_match_launch_authorities_v1(
    gesture_authorization: &RatifiedMtgoCompetitiveDuelGestureAuthorizationV1,
    pass_match_launch: &RatifiedMtgoCompetitiveMatchLaunchV1,
) -> Result<MtgoReviewedCompetitiveGestureRatificationCandidateV1, String> {
    let candidate = review_competitive_duel_gesture_ratification_candidate_from_correspondence_v1(
        &gesture_authorization._permission_correspondence,
        &gesture_authorization.visible_account_alias,
        gesture_authorization.event_kind,
        &gesture_authorization._gesture_profile,
    )?;
    if candidate.ratification_commitment_sha256
        != gesture_authorization.authorization_commitment_sha256
        || candidate.mode_authorization_commitment_sha256
            != gesture_authorization.mode_authorization_commitment_sha256
        || candidate.permission_review_commitment_sha256
            != gesture_authorization.permission_review_commitment_sha256
        || candidate.gesture_evaluation_commitment_sha256
            != gesture_authorization.gesture_evaluation_commitment_sha256
        || candidate.gesture_profile_admission_commitment_sha256
            != gesture_authorization.gesture_profile_admission_commitment_sha256
    {
        return Err("competitive gesture authorization commitment changed".to_owned());
    }
    validate_competitive_match_launch_record_v1(
        &gesture_authorization.scope,
        &pass_match_launch.authorization,
    )?;
    validate_competitive_gesture_match_launch_facts_v1(
        &candidate,
        pass_match_launch,
        &gesture_authorization.visible_account_alias,
    )?;
    Ok(candidate)
}

fn validate_competitive_gesture_match_launch_facts_v1(
    candidate: &MtgoReviewedCompetitiveGestureRatificationCandidateV1,
    pass_match_launch: &RatifiedMtgoCompetitiveMatchLaunchV1,
    visible_account_alias: &str,
) -> Result<(), String> {
    if candidate.supported_action_families != canonical_duel_gesture_action_families_v1()
        || candidate.event_kind != pass_match_launch.authorization.event_kind
        || candidate.mode_authorization_commitment_sha256
            != pass_match_launch.mode_authorization_commitment_sha256
        || candidate.account_alias_sha256 != pass_match_launch.authorization.account_alias_sha256
        || candidate.correspondence_sha256
            != pass_match_launch.authorization.written_permission_sha256
        || format!("{:x}", Sha256::digest(visible_account_alias.as_bytes()))
            != candidate.account_alias_sha256
        || pass_match_launch.valid_from_frame_sequence == 0
        || pass_match_launch.valid_from_frame_sequence
            > pass_match_launch.authorization.valid_through_frame_sequence
    {
        return Err(
            "competitive gesture permission and priority-Pass launch describe different exact games"
                .to_owned(),
        );
    }
    for commitment in [
        candidate.permission_review_commitment_sha256.as_str(),
        candidate.mode_authorization_commitment_sha256.as_str(),
        candidate.gesture_evaluation_commitment_sha256.as_str(),
        candidate
            .gesture_profile_admission_commitment_sha256
            .as_str(),
        candidate.gesture_target_runtime_binary_sha256.as_str(),
        candidate.gesture_target_assets_manifest_sha256.as_str(),
        candidate.ratification_commitment_sha256.as_str(),
        pass_match_launch
            .gameplay_authorization_commitment_sha256
            .as_str(),
        pass_match_launch
            .launch_authorization_commitment_sha256
            .as_str(),
    ] {
        if !is_sha256_v2(commitment) {
            return Err(
                "competitive gesture match launch contains an invalid commitment".to_owned(),
            );
        }
    }
    let expected_gameplay =
        competitive_match_gameplay_authorization_commitment_v1(&pass_match_launch.authorization)
            .map_err(|error| format!("competitive gesture match commitment: {error}"))?;
    let expected_pass_launch = competitive_match_launch_commitment_v1(
        visible_account_alias,
        &candidate.mode_authorization_commitment_sha256,
        &expected_gameplay,
        &pass_match_launch.authorization,
    );
    if expected_gameplay != pass_match_launch.gameplay_authorization_commitment_sha256
        || expected_pass_launch != pass_match_launch.launch_authorization_commitment_sha256
    {
        return Err("priority-Pass launch commitment changed before gesture extension".to_owned());
    }
    Ok(())
}

fn ratify_competitive_gesture_match_launch_from_confirmation_v1(
    gesture_authorization: RatifiedMtgoCompetitiveDuelGestureAuthorizationV1,
    pass_match_launch: RatifiedMtgoCompetitiveMatchLaunchV1,
    candidate: MtgoReviewedCompetitiveGestureRatificationCandidateV1,
    challenge_nonce: [u8; 8],
    issued_at_unix_millis: u128,
    supplied_phrase: &str,
) -> Result<RatifiedMtgoCompetitiveGestureMatchLaunchV1, String> {
    let expected_candidate = validate_competitive_gesture_match_launch_authorities_v1(
        &gesture_authorization,
        &pass_match_launch,
    )?;
    if candidate != expected_candidate {
        return Err("competitive gesture match launch candidate changed".to_owned());
    }
    let gesture_match_launch_commitment_sha256 =
        competitive_gesture_match_launch_commitment_from_facts_v1(
            &candidate,
            &pass_match_launch,
            &gesture_authorization.visible_account_alias,
            &challenge_nonce,
            issued_at_unix_millis,
            supplied_phrase,
        )?;
    Ok(RatifiedMtgoCompetitiveGestureMatchLaunchV1 {
        gesture_authorization,
        pass_match_launch,
        gesture_match_launch_commitment_sha256,
    })
}

fn competitive_gesture_match_launch_commitment_from_facts_v1(
    candidate: &MtgoReviewedCompetitiveGestureRatificationCandidateV1,
    pass_match_launch: &RatifiedMtgoCompetitiveMatchLaunchV1,
    visible_account_alias: &str,
    challenge_nonce: &[u8; 8],
    issued_at_unix_millis: u128,
    supplied_phrase: &str,
) -> Result<String, String> {
    validate_competitive_gesture_match_launch_facts_v1(
        candidate,
        pass_match_launch,
        visible_account_alias,
    )?;
    if issued_at_unix_millis == 0 {
        return Err("attended gesture match launch has an invalid issue time".to_owned());
    }
    let expected_phrase = attended_competitive_gesture_match_launch_confirmation_phrase_v1(
        candidate.event_kind,
        pass_match_launch.authorization.game_number,
        challenge_nonce,
    );
    if supplied_phrase != expected_phrase {
        return Err("attended competitive gesture match launch challenge did not match".to_owned());
    }
    let event_kind: &[u8] = match candidate.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let family_json = serde_json::to_vec(&candidate.supported_action_families)
        .map_err(|error| format!("serialize attended gesture launch families: {error}"))?;
    Ok(hash_parts_v2(
        ATTENDED_COMPETITIVE_GESTURE_MATCH_LAUNCH_UPGRADE_DOMAIN_V1,
        &[
            candidate.ratification_commitment_sha256.as_bytes(),
            candidate.permission_review_commitment_sha256.as_bytes(),
            candidate.mode_authorization_commitment_sha256.as_bytes(),
            candidate.gesture_evaluation_commitment_sha256.as_bytes(),
            candidate
                .gesture_profile_admission_commitment_sha256
                .as_bytes(),
            candidate.gesture_target_runtime_binary_sha256.as_bytes(),
            candidate
                .gesture_target_assets_manifest_sha256
                .as_bytes(),
            pass_match_launch
                .launch_authorization_commitment_sha256
                .as_bytes(),
            pass_match_launch
                .gameplay_authorization_commitment_sha256
                .as_bytes(),
            pass_match_launch
                .authorization
                .owner_launch_authorization_sha256
                .as_bytes(),
            pass_match_launch
                .authorization
                .event_identity_sha256
                .as_bytes(),
            pass_match_launch
                .authorization
                .match_identity_sha256
                .as_bytes(),
            pass_match_launch
                .authorization
                .entry_authorization_sha256
                .as_bytes(),
            event_kind,
            &[pass_match_launch.authorization.game_number],
            pass_match_launch
                .valid_from_frame_sequence
                .to_be_bytes()
                .as_slice(),
            pass_match_launch
                .authorization
                .valid_through_frame_sequence
                .to_be_bytes()
                .as_slice(),
            challenge_nonce.as_slice(),
            issued_at_unix_millis.to_be_bytes().as_slice(),
            supplied_phrase.as_bytes(),
            family_json.as_slice(),
            b"owner_extends_exact_pass_launch_to_complete_reviewed_eleven_family_profile_no_entry_or_spending",
        ],
    ))
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
        if value.is_empty() || value.len() > 32 || !value.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(format!(
                "attended {field} must be a bounded decimal integer"
            ));
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

fn attended_competitive_gesture_match_launch_confirmation_phrase_v1(
    event_kind: MtgoCompetitiveEventKindV1,
    game_number: u8,
    challenge_nonce: &[u8; 8],
) -> String {
    let nonce = challenge_nonce
        .iter()
        .map(|value| format!("{value:02X}"))
        .collect::<String>();
    format!(
        "AUTHORIZE MTGO {} GAME {game_number} ALL ELEVEN GESTURES {nonce}",
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

fn competitive_duel_gesture_ratification_candidate_from_parts_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
    gesture_profile: &CompetitiveDuelGestureProfileFactsV1<'_>,
) -> Result<MtgoReviewedCompetitiveGestureRatificationCandidateV1, String> {
    let permission_review_commitment_sha256 = correspondence.review_commitment_sha256().to_owned();
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(event_kind)
        .map_err(|error| format!("derive reviewed competitive gesture mode scope: {error}"))?;
    let mode_authorization_commitment_sha256 = validate_exact_competitive_mode_authorization_v1(
        &scope,
        visible_account_alias,
        event_kind,
        "competitive gesture permission",
    )?;
    let canonical_families = canonical_duel_gesture_action_families_v1();
    if gesture_profile.supported_action_families != canonical_families.as_slice() {
        return Err(
            "competitive gesture permission requires all eleven action families in canonical order"
                .to_owned(),
        );
    }
    let profile_hashes = [
        gesture_profile.evaluation_commitment_sha256,
        gesture_profile.admission_commitment_sha256,
        gesture_profile.runtime_binary_sha256,
        gesture_profile.assets_manifest_sha256,
    ];
    if profile_hashes.iter().any(|value| !is_sha256_v2(value))
        || profile_hashes
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != profile_hashes.len()
    {
        return Err(
            "competitive gesture profile commitments must be valid and pairwise distinct"
                .to_owned(),
        );
    }
    let family_json = serde_json::to_vec(&canonical_families)
        .map_err(|error| format!("serialize competitive gesture families: {error}"))?;
    let event_kind_bytes: &[u8] = match event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let ratification_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_DUEL_GESTURE_AUTHORIZATION_FROM_REVIEW_DOMAIN_V1,
        &[
            permission_review_commitment_sha256.as_bytes(),
            scope.account_alias_sha256.as_bytes(),
            scope.written_permission_sha256.as_bytes(),
            visible_account_alias.as_bytes(),
            event_kind_bytes,
            mode_authorization_commitment_sha256.as_bytes(),
            gesture_profile.evaluation_commitment_sha256.as_bytes(),
            gesture_profile.admission_commitment_sha256.as_bytes(),
            gesture_profile.runtime_binary_sha256.as_bytes(),
            gesture_profile.assets_manifest_sha256.as_bytes(),
            &family_json,
            b"complete_reviewed_eleven_family_profile_permission_identity_only_no_input_entry_or_spending",
        ],
    );
    Ok(MtgoReviewedCompetitiveGestureRatificationCandidateV1 {
        permission_review_commitment_sha256,
        account_alias_sha256: scope.account_alias_sha256,
        correspondence_sha256: scope.written_permission_sha256,
        mode_authorization_commitment_sha256,
        gesture_evaluation_commitment_sha256: gesture_profile
            .evaluation_commitment_sha256
            .to_owned(),
        gesture_profile_admission_commitment_sha256: gesture_profile
            .admission_commitment_sha256
            .to_owned(),
        gesture_target_runtime_binary_sha256: gesture_profile.runtime_binary_sha256.to_owned(),
        gesture_target_assets_manifest_sha256: gesture_profile.assets_manifest_sha256.to_owned(),
        supported_action_families: canonical_families,
        ratification_commitment_sha256,
        event_kind,
    })
}

fn competitive_entry_ratification_candidate_from_parts_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: &str,
    lifecycle: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    source_identity: &MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1,
    entry_authorization: &MtgoCompetitiveEntryAuthorizationV1,
    review: &MtgoControlBoundCompetitiveEntryReviewCommitmentsV4,
) -> Result<MtgoReviewedCompetitiveEntryRatificationCandidateV1, String> {
    let classifier = &review.classifier_bound_review;
    let source_bound = &classifier.source_bound_review;
    let attended = &source_bound.attended_review;
    let dry_run = &review.entry_control_dry_run;
    let retained_classifier_commitment =
        require_classifier_bound_competitive_entry_source_v3(source_identity)?;
    if classifier.source_navigation_classification_result_commitment_sha256
        != retained_classifier_commitment
        || source_bound.source_identity_commitment_sha256
            != source_identity.source_identity_commitment_sha256
        || source_bound.source_capture_commitment_sha256
            != source_identity.source_capture_commitment_sha256
        || source_bound
            .source_navigation_classification_result_commitment_sha256
            .as_deref()
            != Some(retained_classifier_commitment)
        || source_bound
            .attended_review
            .source_lifecycle_snapshot_commitment_sha256
            != source_identity.source_lifecycle_snapshot_commitment_sha256
        || source_identity.source_lifecycle_snapshot_commitment_sha256
            != lifecycle.snapshot_commitment_sha256()
        || source_identity.event_kind != lifecycle.event_kind()
        || source_identity.frame_id != lifecycle.frame_id_v1()
        || source_identity.frame_sequence != lifecycle.frame_sequence()
    {
        return Err("competitive entry ratification changed the exact source lineage".to_owned());
    }
    let expected_classifier_bound = classifier_bound_competitive_entry_review_commitment_v3(
        source_bound,
        retained_classifier_commitment,
    );
    if classifier.classifier_bound_review_commitment_sha256 != expected_classifier_bound {
        return Err(
            "competitive entry ratification changed the classifier-bound review".to_owned(),
        );
    }
    let expected_control_bound =
        bind_control_bound_competitive_entry_review_commitment_v4(classifier, dry_run)?;
    if review.control_bound_review_commitment_sha256 != expected_control_bound {
        return Err("competitive entry ratification changed the control-bound review".to_owned());
    }

    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(attended.event_kind)
        .map_err(|error| format!("derive reviewed competitive entry mode scope: {error}"))?;
    let mode_authorization_commitment_sha256 = validate_competitive_duel_pass_authorization_v1(
        &scope,
        visible_account_alias,
        attended.event_kind,
    )?;
    if attended.permission_review_commitment_sha256 != correspondence.review_commitment_sha256()
        || attended.mode_authorization_commitment_sha256 != mode_authorization_commitment_sha256
        || attended.event_kind != lifecycle.event_kind()
        || attended.frame_id != lifecycle.frame_id_v1()
        || attended.frame_sequence != lifecycle.frame_sequence()
    {
        return Err(
            "competitive entry ratification changed the correspondence, account, mode, or frame"
                .to_owned(),
        );
    }
    let lifecycle_intent = make_offline_competitive_lifecycle_intent_v1(
        lifecycle,
        MtgoCompetitiveLifecycleActionV1::ConfirmEntry,
        &scope,
        Some(entry_authorization),
    )
    .map_err(|error| format!("validate reviewed competitive entry authorization: {error}"))?;
    let entry_authorization_sha256 = lifecycle_intent
        .entry_authorization_sha256
        .ok_or("competitive entry ratification has no exact entry authorization")?;
    let event_identity_sha256 = lifecycle
        .event_identity_sha256_v1()
        .ok_or("competitive entry ratification has no exact event identity")?
        .to_owned();
    let entry_terms = lifecycle
        .entry_terms_v1()
        .ok_or("competitive entry ratification has no exact visible entry terms")?;
    if attended.entry_authorization_sha256 != entry_authorization_sha256
        || attended.resource != entry_terms.resource
        || attended.amount != entry_terms.amount
        || source_identity.resource != entry_terms.resource
        || source_identity.amount != entry_terms.amount
        || dry_run.event_kind != attended.event_kind
        || dry_run.frame_id != attended.frame_id
        || dry_run.frame_sequence != attended.frame_sequence
        || dry_run.resource != entry_terms.resource
        || dry_run.amount != entry_terms.amount
        || !dry_run.visibly_enabled_confirmed
    {
        return Err(
            "competitive entry ratification changed the visible event, terms, or enabled control"
                .to_owned(),
        );
    }
    for digest in [
        correspondence.review_commitment_sha256(),
        scope.account_alias_sha256.as_str(),
        scope.written_permission_sha256.as_str(),
        mode_authorization_commitment_sha256.as_str(),
        review.control_bound_review_commitment_sha256.as_str(),
        classifier
            .classifier_bound_review_commitment_sha256
            .as_str(),
        attended.owner_review_receipt_sha256.as_str(),
        entry_authorization_sha256.as_str(),
        source_identity.source_identity_commitment_sha256.as_str(),
        source_identity.source_capture_commitment_sha256.as_str(),
        retained_classifier_commitment,
        source_identity
            .source_lifecycle_snapshot_commitment_sha256
            .as_str(),
        dry_run.dry_run_commitment_sha256.as_str(),
        dry_run.visible_control_label_sha256.as_str(),
        dry_run.visible_control_region_sha256.as_str(),
        event_identity_sha256.as_str(),
        entry_terms.terms_sha256.as_str(),
    ] {
        if !is_sha256_v2(digest) {
            return Err("competitive entry ratification contains an invalid commitment".to_owned());
        }
    }
    let event_kind: &[u8] = match attended.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let resource: &[u8] = match entry_terms.resource {
        MtgoCompetitiveEntryResourceV1::NoCost => b"no_cost",
        MtgoCompetitiveEntryResourceV1::ExistingEventToken => b"existing_event_token",
        MtgoCompetitiveEntryResourceV1::ExistingPlayPoints => b"existing_play_points",
        MtgoCompetitiveEntryResourceV1::ExistingEventTickets => b"existing_event_tickets",
    };
    let ratification_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_ENTRY_AUTHORIZATION_RATIFICATION_DOMAIN_V1,
        &[
            correspondence.review_commitment_sha256().as_bytes(),
            scope.account_alias_sha256.as_bytes(),
            scope.written_permission_sha256.as_bytes(),
            mode_authorization_commitment_sha256.as_bytes(),
            review.control_bound_review_commitment_sha256.as_bytes(),
            classifier
                .classifier_bound_review_commitment_sha256
                .as_bytes(),
            attended.owner_review_receipt_sha256.as_bytes(),
            entry_authorization_sha256.as_bytes(),
            source_identity.source_identity_commitment_sha256.as_bytes(),
            source_identity.source_capture_commitment_sha256.as_bytes(),
            retained_classifier_commitment.as_bytes(),
            source_identity
                .source_lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            dry_run.dry_run_commitment_sha256.as_bytes(),
            dry_run.visible_control_label_sha256.as_bytes(),
            dry_run.visible_control_region_sha256.as_bytes(),
            event_kind,
            event_identity_sha256.as_bytes(),
            entry_terms.terms_sha256.as_bytes(),
            resource,
            entry_terms.amount.to_be_bytes().as_slice(),
            b"exact_owner_reviewed_existing_account_resource_entry_requires_fresh_recapture_and_visible_postcondition",
        ],
    );
    Ok(MtgoReviewedCompetitiveEntryRatificationCandidateV1 {
        permission_review_commitment_sha256: correspondence.review_commitment_sha256().to_owned(),
        account_alias_sha256: scope.account_alias_sha256,
        correspondence_sha256: scope.written_permission_sha256,
        mode_authorization_commitment_sha256,
        control_bound_review_commitment_sha256: review
            .control_bound_review_commitment_sha256
            .clone(),
        owner_review_receipt_sha256: attended.owner_review_receipt_sha256.clone(),
        entry_authorization_sha256,
        source_identity_commitment_sha256: source_identity
            .source_identity_commitment_sha256
            .clone(),
        source_capture_commitment_sha256: source_identity.source_capture_commitment_sha256.clone(),
        source_navigation_classification_result_commitment_sha256: retained_classifier_commitment
            .to_owned(),
        visible_control_region_sha256: dry_run.visible_control_region_sha256.clone(),
        event_identity_sha256,
        entry_terms_sha256: entry_terms.terms_sha256.clone(),
        ratification_commitment_sha256,
        event_kind: attended.event_kind,
        resource: entry_terms.resource,
        amount: entry_terms.amount,
    })
}

fn competitive_entry_preparation_from_commitments_v1(
    authorization: &MtgoReviewedCompetitiveEntryRatificationCandidateV1,
    recapture: &MtgoCompetitiveEntryImmediateRecaptureCommitmentsV1,
) -> Result<MtgoPreparedCompetitiveEntryCommitmentsV1, String> {
    if authorization.account_alias_sha256 != recapture.approved_account_alias_sha256
        || authorization.source_identity_commitment_sha256
            != recapture.source_identity_commitment_sha256
        || authorization.source_capture_commitment_sha256
            != recapture.source_capture_commitment_sha256
        || authorization.source_navigation_classification_result_commitment_sha256
            != recapture.source_classification_result_commitment_sha256
        || authorization.visible_control_region_sha256 != recapture.visible_control_region_sha256
        || authorization.event_identity_sha256 != recapture.event_identity_sha256
        || authorization.entry_terms_sha256 != recapture.entry_terms_sha256
        || authorization.event_kind != recapture.event_kind
        || authorization.resource != recapture.resource
        || authorization.amount != recapture.amount
    {
        return Err(
            "competitive entry preparation changed the ratified account, source, control, event, or terms"
                .to_owned(),
        );
    }
    if recapture.source_frame_id == 0
        || recapture.source_frame_sequence == 0
        || recapture.source_captured_at_unix_millis == 0
        || recapture.immediate_frame_id == 0
        || recapture.immediate_frame_sequence <= recapture.source_frame_sequence
        || recapture.immediate_captured_at_unix_millis <= recapture.source_captured_at_unix_millis
    {
        return Err(
            "competitive entry preparation does not retain a strictly newer frame identity"
                .to_owned(),
        );
    }
    for digest in [
        authorization.ratification_commitment_sha256.as_str(),
        recapture.navigation_profile_commitment_sha256.as_str(),
        recapture
            .navigation_profile_admission_commitment_sha256
            .as_str(),
        recapture.approved_account_alias_sha256.as_str(),
        recapture.runtime_identity_commitment_sha256.as_str(),
        recapture.window_continuity_commitment_sha256.as_str(),
        recapture.source_identity_commitment_sha256.as_str(),
        recapture.source_capture_commitment_sha256.as_str(),
        recapture
            .source_classification_result_commitment_sha256
            .as_str(),
        recapture
            .source_lifecycle_snapshot_commitment_sha256
            .as_str(),
        recapture.immediate_capture_commitment_sha256.as_str(),
        recapture
            .immediate_classification_result_commitment_sha256
            .as_str(),
        recapture
            .immediate_lifecycle_snapshot_commitment_sha256
            .as_str(),
        recapture.event_label_region_sha256.as_str(),
        recapture.visible_control_label_sha256.as_str(),
        recapture.visible_control_region_sha256.as_str(),
        recapture.event_identity_sha256.as_str(),
        recapture.entry_terms_sha256.as_str(),
        recapture.recapture_commitment_sha256.as_str(),
    ] {
        if !is_sha256_v2(digest) {
            return Err("competitive entry preparation contains an invalid commitment".to_owned());
        }
    }
    let event_kind: &[u8] = match authorization.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let resource: &[u8] = match authorization.resource {
        MtgoCompetitiveEntryResourceV1::NoCost => b"no_cost",
        MtgoCompetitiveEntryResourceV1::ExistingEventToken => b"existing_event_token",
        MtgoCompetitiveEntryResourceV1::ExistingPlayPoints => b"existing_play_points",
        MtgoCompetitiveEntryResourceV1::ExistingEventTickets => b"existing_event_tickets",
    };
    let preparation_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_ENTRY_PREPARATION_DOMAIN_V1,
        &[
            authorization.ratification_commitment_sha256.as_bytes(),
            recapture.recapture_commitment_sha256.as_bytes(),
            recapture.navigation_profile_commitment_sha256.as_bytes(),
            recapture
                .navigation_profile_admission_commitment_sha256
                .as_bytes(),
            recapture.approved_account_alias_sha256.as_bytes(),
            recapture.runtime_identity_commitment_sha256.as_bytes(),
            recapture.window_continuity_commitment_sha256.as_bytes(),
            recapture.source_identity_commitment_sha256.as_bytes(),
            recapture.source_capture_commitment_sha256.as_bytes(),
            recapture
                .source_classification_result_commitment_sha256
                .as_bytes(),
            recapture
                .source_lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            recapture.immediate_capture_commitment_sha256.as_bytes(),
            recapture
                .immediate_classification_result_commitment_sha256
                .as_bytes(),
            recapture
                .immediate_lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            recapture.event_label_region_sha256.as_bytes(),
            recapture.visible_control_label_sha256.as_bytes(),
            recapture.visible_control_region_sha256.as_bytes(),
            event_kind,
            recapture.event_identity_sha256.as_bytes(),
            recapture.entry_terms_sha256.as_bytes(),
            resource,
            authorization.amount.to_be_bytes().as_slice(),
            recapture.source_frame_id.to_be_bytes().as_slice(),
            recapture.source_frame_sequence.to_be_bytes().as_slice(),
            recapture
                .source_captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            recapture.immediate_frame_id.to_be_bytes().as_slice(),
            recapture.immediate_frame_sequence.to_be_bytes().as_slice(),
            recapture
                .immediate_captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            b"ratified_exact_entry_fresh_visible_review_retained_no_input_no_join_no_spending",
        ],
    );
    Ok(MtgoPreparedCompetitiveEntryCommitmentsV1 {
        entry_ratification_commitment_sha256: authorization.ratification_commitment_sha256.clone(),
        immediate_recapture: recapture.clone(),
        preparation_commitment_sha256,
        event_kind: authorization.event_kind,
        resource: authorization.resource,
        amount: authorization.amount,
        immediate_frame_id: recapture.immediate_frame_id,
        immediate_frame_sequence: recapture.immediate_frame_sequence,
    })
}

fn ratify_competitive_entry_authorization_with_commitment_v1(
    correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    review: CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4,
    visible_account_alias: String,
    ratified_commitment_sha256: Option<&str>,
) -> Result<RatifiedMtgoCompetitiveEntryAuthorizationV1, String> {
    let candidate = review_competitive_entry_ratification_candidate_v1(
        &correspondence,
        &review,
        &visible_account_alias,
    )?;
    if ratified_commitment_sha256 != Some(candidate.ratification_commitment_sha256.as_str()) {
        return Err(
            "the exact owner-reviewed competitive entry is not ratified in this build".to_owned(),
        );
    }
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(candidate.event_kind)
        .map_err(|error| format!("derive ratified competitive entry scope: {error}"))?;
    Ok(RatifiedMtgoCompetitiveEntryAuthorizationV1 {
        _permission_correspondence: correspondence,
        _review: review,
        _scope: scope,
        visible_account_alias,
        commitments: candidate,
    })
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
    validate_exact_competitive_mode_authorization_v1(
        authorization,
        visible_account_alias,
        event_kind,
        "competitive Pass ratification",
    )
}

fn validate_exact_competitive_mode_authorization_v1(
    authorization: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
    context: &str,
) -> Result<String, String> {
    let runtime_mode = match event_kind {
        MtgoCompetitiveEventKindV1::League => MtgoRuntimeModeV1::LeagueInput,
        MtgoCompetitiveEventKindV1::Challenge => MtgoRuntimeModeV1::ChallengeInput,
    };
    validate_authorization_for_mode_v1(authorization, runtime_mode)
        .map_err(|error| format!("{context} authorization rejected: {error}"))?;
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
        return Err(format!(
            "{context} requires exactly one League or Challenge input mode"
        ));
    }
    competitive_mode_authorization_commitment_v1(authorization, event_kind)
        .map_err(|error| format!("{context} mode commitment rejected: {error}"))
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

fn initial_competitive_gesture_game_session_commitment_v1(
    candidate: &MtgoReviewedCompetitiveGestureRatificationCandidateV1,
    pass_match_launch: &RatifiedMtgoCompetitiveMatchLaunchV1,
    gesture_match_launch_commitment_sha256: &str,
) -> String {
    let event_kind: &[u8] = match candidate.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    hash_parts_v2(
        COMPETITIVE_GESTURE_GAME_SESSION_INITIAL_DOMAIN_V1,
        &[
            candidate.ratification_commitment_sha256.as_bytes(),
            candidate.gesture_evaluation_commitment_sha256.as_bytes(),
            candidate
                .gesture_profile_admission_commitment_sha256
                .as_bytes(),
            pass_match_launch
                .launch_authorization_commitment_sha256
                .as_bytes(),
            pass_match_launch
                .gameplay_authorization_commitment_sha256
                .as_bytes(),
            gesture_match_launch_commitment_sha256.as_bytes(),
            event_kind,
            &[pass_match_launch.authorization.game_number],
            pass_match_launch
                .valid_from_frame_sequence
                .to_be_bytes()
                .as_slice(),
            pass_match_launch
                .authorization
                .valid_through_frame_sequence
                .to_be_bytes()
                .as_slice(),
            b"move_only_all_family_lineage_no_preparation_execution_entry_or_spending",
        ],
    )
}

fn competitive_duel_gesture_sequence_session_binding_from_parts_v1(
    sequence: &MtgoOpaqueCompetitiveDuelGestureSequenceCommitmentsV1,
    session: &MtgoCompetitiveGestureGameSessionCommitmentsV1,
) -> Result<MtgoSessionBoundCompetitiveDuelGestureCommitmentsV1, String> {
    for commitment in [
        sequence.competitive_action_plan_commitment_sha256.as_str(),
        sequence.gesture_plan_commitment_sha256.as_str(),
        sequence
            .competitive_mode_authorization_commitment_sha256
            .as_str(),
        sequence
            .competitive_match_gameplay_authorization_commitment_sha256
            .as_str(),
        sequence.current_stage_binding_commitment_sha256.as_str(),
        sequence.current_opaque_stage_commitment_sha256.as_str(),
        sequence.sequence_commitment_sha256.as_str(),
        session.session_commitment_sha256.as_str(),
        session
            .general_gesture_permission_commitment_sha256
            .as_str(),
        session.mode_authorization_commitment_sha256.as_str(),
        session.pass_match_launch_commitment_sha256.as_str(),
        session
            .match_gameplay_authorization_commitment_sha256
            .as_str(),
        session.gesture_match_launch_commitment_sha256.as_str(),
        session.gesture_evaluation_commitment_sha256.as_str(),
        session.gesture_profile_admission_commitment_sha256.as_str(),
    ] {
        if !is_sha256_v2(commitment) {
            return Err("gesture session binding contains an invalid commitment".to_owned());
        }
    }
    if sequence.last_visible_transition_commitment_sha256.is_some()
        || sequence.current_stage_index != 0
        || sequence.observed_stage_count != 1
        || sequence.gesture_stage_count == 0
        || sequence.gesture_stage_count > 64
        || sequence.current_frame_id == 0
        || sequence.current_frame_sequence == 0
        || !canonical_duel_gesture_action_families_v1().contains(&sequence.selected_action_family)
    {
        return Err(
            "gesture session binding requires one canonical source stage of a complete plan"
                .to_owned(),
        );
    }
    if sequence.event_kind != session.event_kind
        || sequence.game_number != session.game_number
        || sequence.competitive_mode_authorization_commitment_sha256
            != session.mode_authorization_commitment_sha256
        || sequence.competitive_match_gameplay_authorization_commitment_sha256
            != session.match_gameplay_authorization_commitment_sha256
        || sequence.gameplay_authorization_valid_through_frame_sequence
            != session.valid_through_frame_sequence
        || sequence.current_frame_sequence < session.valid_from_frame_sequence
        || sequence.current_frame_sequence <= session.last_confirmed_frame_sequence
        || sequence.current_frame_sequence > session.valid_through_frame_sequence
    {
        return Err(
            "gesture sequence does not match the exact game session mode, game, or frame lifetime"
                .to_owned(),
        );
    }
    let family_json = serde_json::to_vec(&sequence.selected_action_family)
        .map_err(|error| format!("serialize session-bound gesture family: {error}"))?;
    let event_kind: &[u8] = match sequence.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let binding_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_GESTURE_SESSION_SEQUENCE_BINDING_DOMAIN_V1,
        &[
            session.session_commitment_sha256.as_bytes(),
            session
                .general_gesture_permission_commitment_sha256
                .as_bytes(),
            session.gesture_match_launch_commitment_sha256.as_bytes(),
            session.gesture_evaluation_commitment_sha256.as_bytes(),
            session
                .gesture_profile_admission_commitment_sha256
                .as_bytes(),
            sequence
                .competitive_action_plan_commitment_sha256
                .as_bytes(),
            sequence.gesture_plan_commitment_sha256.as_bytes(),
            sequence.current_stage_binding_commitment_sha256.as_bytes(),
            sequence.current_opaque_stage_commitment_sha256.as_bytes(),
            sequence.sequence_commitment_sha256.as_bytes(),
            family_json.as_slice(),
            event_kind,
            &[sequence.game_number],
            sequence.current_frame_sequence.to_be_bytes().as_slice(),
            sequence.gesture_stage_count.to_be_bytes().as_slice(),
            session.confirmed_action_count.to_be_bytes().as_slice(),
            b"one_source_stage_bound_to_exact_all_family_session_no_preparation_execution_or_input",
        ],
    );
    Ok(MtgoSessionBoundCompetitiveDuelGestureCommitmentsV1 {
        binding_commitment_sha256,
        game_session_commitment_sha256: session.session_commitment_sha256.clone(),
        gesture_match_launch_commitment_sha256: session
            .gesture_match_launch_commitment_sha256
            .clone(),
        gesture_evaluation_commitment_sha256: session.gesture_evaluation_commitment_sha256.clone(),
        gesture_profile_admission_commitment_sha256: session
            .gesture_profile_admission_commitment_sha256
            .clone(),
        competitive_action_plan_commitment_sha256: sequence
            .competitive_action_plan_commitment_sha256
            .clone(),
        gesture_plan_commitment_sha256: sequence.gesture_plan_commitment_sha256.clone(),
        gesture_sequence_commitment_sha256: sequence.sequence_commitment_sha256.clone(),
        selected_action_family: sequence.selected_action_family,
        event_kind: sequence.event_kind,
        game_number: sequence.game_number,
        source_frame_sequence: sequence.current_frame_sequence,
        gesture_stage_count: sequence.gesture_stage_count,
    })
}

fn competitive_duel_gesture_source_preparation_from_parts_v1(
    bound: &MtgoSessionBoundCompetitiveDuelGestureCommitmentsV1,
    session: &MtgoCompetitiveGestureGameSessionCommitmentsV1,
    prepared: &MtgoOpaqueCompetitiveDuelGestureSourcePreparationCommitmentsV1,
) -> Result<MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1, String> {
    for commitment in [
        bound.binding_commitment_sha256.as_str(),
        bound.game_session_commitment_sha256.as_str(),
        bound.gesture_match_launch_commitment_sha256.as_str(),
        bound.gesture_sequence_commitment_sha256.as_str(),
        bound.competitive_action_plan_commitment_sha256.as_str(),
        bound.gesture_plan_commitment_sha256.as_str(),
        session.session_commitment_sha256.as_str(),
        session.mode_authorization_commitment_sha256.as_str(),
        session
            .match_gameplay_authorization_commitment_sha256
            .as_str(),
        prepared.source_sequence_commitment_sha256.as_str(),
        prepared.competitive_action_plan_commitment_sha256.as_str(),
        prepared.gesture_plan_commitment_sha256.as_str(),
        prepared
            .competitive_mode_authorization_commitment_sha256
            .as_str(),
        prepared
            .competitive_match_gameplay_authorization_commitment_sha256
            .as_str(),
        prepared.fresh_stage_binding_commitment_sha256.as_str(),
        prepared.fresh_capture_commitment_sha256.as_str(),
        prepared.fresh_perception_result_commitment_sha256.as_str(),
        prepared
            .gesture_target_runtime_identity_commitment_sha256
            .as_str(),
        prepared.gesture_target_request_commitment_sha256.as_str(),
        prepared.primitive_commitment_sha256.as_str(),
        prepared.preparation_commitment_sha256.as_str(),
    ] {
        if !is_sha256_v2(commitment) {
            return Err("gesture source preparation contains an invalid commitment".to_owned());
        }
    }
    let expected_fresh_sequence = bound
        .source_frame_sequence
        .checked_add(1)
        .ok_or("gesture source preparation sequence overflow")?;
    if bound.game_session_commitment_sha256 != session.session_commitment_sha256
        || prepared.source_sequence_commitment_sha256 != bound.gesture_sequence_commitment_sha256
        || prepared.competitive_action_plan_commitment_sha256
            != bound.competitive_action_plan_commitment_sha256
        || prepared.gesture_plan_commitment_sha256 != bound.gesture_plan_commitment_sha256
        || prepared.competitive_mode_authorization_commitment_sha256
            != session.mode_authorization_commitment_sha256
        || prepared.competitive_match_gameplay_authorization_commitment_sha256
            != session.match_gameplay_authorization_commitment_sha256
        || prepared.selected_action_family != bound.selected_action_family
        || prepared.event_kind != bound.event_kind
        || prepared.event_kind != session.event_kind
        || prepared.game_number != bound.game_number
        || prepared.game_number != session.game_number
        || prepared.stage_index != 0
        || prepared.gesture_stage_count != bound.gesture_stage_count
        || prepared.target_count == 0
        || prepared.target_count > 2
        || prepared.fresh_frame_id == 0
        || prepared.fresh_frame_sequence != expected_fresh_sequence
        || prepared.fresh_frame_sequence <= session.last_confirmed_frame_sequence
        || prepared.fresh_frame_sequence < session.valid_from_frame_sequence
        || prepared.fresh_frame_sequence > session.valid_through_frame_sequence
        || prepared.gameplay_authorization_valid_through_frame_sequence
            != session.valid_through_frame_sequence
        || prepared.fresh_captured_at_unix_millis == 0
    {
        return Err(
            "fresh gesture source stage does not match the exact competitive session".to_owned(),
        );
    }
    let family_json = serde_json::to_vec(&prepared.selected_action_family)
        .map_err(|error| format!("serialize prepared session gesture family: {error}"))?;
    let event_kind_json = serde_json::to_vec(&prepared.event_kind)
        .map_err(|error| format!("serialize prepared session event kind: {error}"))?;
    let preparation_binding_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_GESTURE_SESSION_SOURCE_PREPARATION_DOMAIN_V1,
        &[
            bound.binding_commitment_sha256.as_bytes(),
            session.session_commitment_sha256.as_bytes(),
            session.gesture_match_launch_commitment_sha256.as_bytes(),
            prepared.source_sequence_commitment_sha256.as_bytes(),
            prepared
                .competitive_action_plan_commitment_sha256
                .as_bytes(),
            prepared.gesture_plan_commitment_sha256.as_bytes(),
            prepared.fresh_stage_binding_commitment_sha256.as_bytes(),
            prepared.fresh_capture_commitment_sha256.as_bytes(),
            prepared
                .fresh_perception_result_commitment_sha256
                .as_bytes(),
            prepared
                .gesture_target_runtime_identity_commitment_sha256
                .as_bytes(),
            prepared.gesture_target_request_commitment_sha256.as_bytes(),
            prepared.primitive_commitment_sha256.as_bytes(),
            prepared.preparation_commitment_sha256.as_bytes(),
            &family_json,
            &event_kind_json,
            &[prepared.game_number],
            &prepared.stage_index.to_be_bytes(),
            &prepared.gesture_stage_count.to_be_bytes(),
            &prepared.target_count.to_be_bytes(),
            &prepared.fresh_frame_id.to_be_bytes(),
            &prepared.fresh_frame_sequence.to_be_bytes(),
            b"source_primitive_freshly_rechecked_pinned_runtime_targets_no_input",
        ],
    );
    Ok(MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1 {
        preparation_binding_commitment_sha256,
        session_sequence_binding_commitment_sha256: bound.binding_commitment_sha256.clone(),
        game_session_commitment_sha256: session.session_commitment_sha256.clone(),
        gesture_match_launch_commitment_sha256: session
            .gesture_match_launch_commitment_sha256
            .clone(),
        source_sequence_commitment_sha256: prepared.source_sequence_commitment_sha256.clone(),
        competitive_action_plan_commitment_sha256: prepared
            .competitive_action_plan_commitment_sha256
            .clone(),
        gesture_plan_commitment_sha256: prepared.gesture_plan_commitment_sha256.clone(),
        fresh_stage_binding_commitment_sha256: prepared
            .fresh_stage_binding_commitment_sha256
            .clone(),
        fresh_capture_commitment_sha256: prepared.fresh_capture_commitment_sha256.clone(),
        fresh_perception_result_commitment_sha256: prepared
            .fresh_perception_result_commitment_sha256
            .clone(),
        gesture_target_runtime_identity_commitment_sha256: prepared
            .gesture_target_runtime_identity_commitment_sha256
            .clone(),
        gesture_target_request_commitment_sha256: prepared
            .gesture_target_request_commitment_sha256
            .clone(),
        primitive_commitment_sha256: prepared.primitive_commitment_sha256.clone(),
        selected_action_family: prepared.selected_action_family,
        event_kind: prepared.event_kind,
        game_number: prepared.game_number,
        stage_index: prepared.stage_index,
        gesture_stage_count: prepared.gesture_stage_count,
        target_count: prepared.target_count,
        fresh_frame_id: prepared.fresh_frame_id,
        fresh_frame_sequence: prepared.fresh_frame_sequence,
        fresh_captured_at_unix_millis: prepared.fresh_captured_at_unix_millis,
    })
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

fn competitive_entry_input_receipt_v1(
    prepared: &OpaqueMtgoPreparedCompetitiveEntryV1,
    input_sent_at_unix_millis: u128,
    cursor_parked_outside_client: bool,
) -> String {
    let prepared_commitments = prepared.commitments_v1();
    competitive_entry_input_receipt_from_parts_v1(
        &prepared_commitments,
        &prepared.pointer_target,
        input_sent_at_unix_millis,
        cursor_parked_outside_client,
    )
}

fn competitive_entry_input_receipt_from_parts_v1(
    prepared: &MtgoPreparedCompetitiveEntryCommitmentsV1,
    target: &MtgoCompetitiveEntryPointerTargetV1,
    input_sent_at_unix_millis: u128,
    cursor_parked_outside_client: bool,
) -> String {
    hash_parts_v2(
        COMPETITIVE_ENTRY_INPUT_RECEIPT_DOMAIN_V1,
        &[
            prepared.preparation_commitment_sha256.as_bytes(),
            prepared
                .entry_ratification_commitment_sha256
                .as_bytes(),
            prepared
                .immediate_recapture
                .recapture_commitment_sha256
                .as_bytes(),
            prepared
                .immediate_recapture
                .visible_control_region_sha256
                .as_bytes(),
            target.hwnd.to_be_bytes().as_slice(),
            target.process_id.to_be_bytes().as_slice(),
            target
                .process_start_filetime_100ns
                .to_be_bytes()
                .as_slice(),
            target.target_x_desktop_px.to_be_bytes().as_slice(),
            target.target_y_desktop_px.to_be_bytes().as_slice(),
            target.park_x_desktop_px.to_be_bytes().as_slice(),
            target.park_y_desktop_px.to_be_bytes().as_slice(),
            input_sent_at_unix_millis.to_be_bytes().as_slice(),
            &[u8::from(cursor_parked_outside_client)],
            b"exactly_one_exactly_ratified_competitive_entry_left_click_pending_visible_confirmation",
        ],
    )
}

fn competitive_entry_confirmation_receipt_v1(
    input: &MtgoCompetitiveEntryInputReceiptCommitmentsV1,
    prepared: &MtgoPreparedCompetitiveEntryCommitmentsV1,
    visible: &MtgoCompetitiveEntryVisibleConfirmationCommitmentsV1,
) -> String {
    let event_kind: &[u8] = match prepared.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let resource: &[u8] = match prepared.resource {
        MtgoCompetitiveEntryResourceV1::NoCost => b"no_cost",
        MtgoCompetitiveEntryResourceV1::ExistingEventToken => b"existing_event_token",
        MtgoCompetitiveEntryResourceV1::ExistingPlayPoints => b"existing_play_points",
        MtgoCompetitiveEntryResourceV1::ExistingEventTickets => b"existing_event_tickets",
    };
    hash_parts_v2(
        COMPETITIVE_ENTRY_CONFIRMATION_RECEIPT_DOMAIN_V1,
        &[
            input.input_receipt_sha256.as_bytes(),
            prepared.preparation_commitment_sha256.as_bytes(),
            prepared.entry_ratification_commitment_sha256.as_bytes(),
            visible
                .frame_transition
                .transition_commitment_sha256
                .as_bytes(),
            visible.confirmation_commitment_sha256.as_bytes(),
            visible.after_capture_commitment_sha256.as_bytes(),
            visible
                .after_classification_result_commitment_sha256
                .as_bytes(),
            visible
                .after_lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            event_kind,
            prepared
                .immediate_recapture
                .event_identity_sha256
                .as_bytes(),
            prepared.immediate_recapture.entry_terms_sha256.as_bytes(),
            resource,
            prepared.amount.to_be_bytes().as_slice(),
            input.input_sent_at_unix_millis.to_be_bytes().as_slice(),
            visible
                .after_captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            visible
                .frame_transition
                .after_frame_id
                .to_be_bytes()
                .as_slice(),
            visible
                .frame_transition
                .after_frame_sequence
                .to_be_bytes()
                .as_slice(),
            visible
                .postcondition_candidate_count
                .to_be_bytes()
                .as_slice(),
            b"one_entry_input_visible_entered_waiting_confirmed_shared_gate_released",
        ],
    )
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

impl VerifiedPointerTargetV3 for OpaqueMtgoPreparedCompetitiveEntryV1 {
    fn hwnd_v3(&self) -> u64 {
        self.pointer_target.hwnd
    }

    fn process_id_v3(&self) -> u32 {
        self.pointer_target.process_id
    }

    fn process_start_filetime_100ns_v3(&self) -> u64 {
        self.pointer_target.process_start_filetime_100ns
    }

    fn dpi_v3(&self) -> u32 {
        self.pointer_target.dpi
    }

    fn client_rect_desktop_px_v3(&self) -> &crate::SignedRectV1 {
        &self.pointer_target.client_rect_desktop_px
    }

    fn target_x_desktop_px_v3(&self) -> i32 {
        self.pointer_target.target_x_desktop_px
    }

    fn target_y_desktop_px_v3(&self) -> i32 {
        self.pointer_target.target_y_desktop_px
    }

    fn park_x_desktop_px_v3(&self) -> i32 {
        self.pointer_target.park_x_desktop_px
    }

    fn park_y_desktop_px_v3(&self) -> i32 {
        self.pointer_target.park_y_desktop_px
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
        check_untrusted_authorization_correspondence_v1,
        validate_visible_competitive_lifecycle_snapshot_v1,
        MtgoAuthorizationCorrespondenceReviewV1, MtgoLifecycleVisibleFactKindV1,
        MtgoLifecycleVisibleFactV1, MtgoRectPxV1, MtgoVisibleCompetitiveLifecycleSnapshotV1,
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

    fn checked_competitive_entry_review_snapshot_v1(
        event_kind: MtgoCompetitiveEventKindV1,
        resource: MtgoCompetitiveEntryResourceV1,
        amount: u32,
    ) -> CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
        validate_visible_competitive_lifecycle_snapshot_v1(
            MtgoVisibleCompetitiveLifecycleSnapshotV1 {
                schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
                snapshot_id: "attended-entry-review-test-v1".to_owned(),
                event_kind,
                phase: MtgoCompetitiveLifecyclePhaseV1::EntryReview,
                frame_id: 17,
                frame_sequence: 41,
                frame_sha256: "1".repeat(64),
                client_bounds: MtgoRectPxV1 {
                    x: 0,
                    y: 0,
                    width: 100,
                    height: 100,
                },
                event_identity_sha256: Some("2".repeat(64)),
                match_identity_sha256: None,
                game_number: None,
                entry_terms: Some(MtgoCompetitiveEntryTermsV1 {
                    terms_sha256: "3".repeat(64),
                    resource,
                    amount,
                }),
                visible_state_complete: true,
                facts: vec![
                    MtgoLifecycleVisibleFactV1 {
                        kind: MtgoLifecycleVisibleFactKindV1::EntryReviewVisible,
                        rect_client_px: MtgoRectPxV1 {
                            x: 10,
                            y: 10,
                            width: 30,
                            height: 20,
                        },
                        content_sha256: "4".repeat(64),
                        confidence_bps: 10_000,
                    },
                    MtgoLifecycleVisibleFactV1 {
                        kind: MtgoLifecycleVisibleFactKindV1::EntryTermsVisible,
                        rect_client_px: MtgoRectPxV1 {
                            x: 10,
                            y: 40,
                            width: 40,
                            height: 20,
                        },
                        content_sha256: "5".repeat(64),
                        confidence_bps: 10_000,
                    },
                ],
            },
        )
        .unwrap()
    }

    fn entry_source_identity_commitments_v3(
        classifier_commitment_sha256: Option<String>,
    ) -> MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1 {
        MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1 {
            source_capture_commitment_sha256: "1".repeat(64),
            source_lifecycle_snapshot_commitment_sha256: "2".repeat(64),
            source_window_title_sha256: "3".repeat(64),
            event_label_region_sha256: "4".repeat(64),
            source_identity_commitment_sha256: "5".repeat(64),
            source_navigation_classification_result_commitment_sha256: classifier_commitment_sha256,
            event_kind: MtgoCompetitiveEventKindV1::League,
            frame_id: 17,
            frame_sequence: 41,
            resource: MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
            amount: 100,
        }
    }

    fn source_bound_entry_review_commitments_v3(
        classifier_commitment_sha256: String,
    ) -> MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2 {
        MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2 {
            source_identity_commitment_sha256: "5".repeat(64),
            source_capture_commitment_sha256: "1".repeat(64),
            source_navigation_classification_result_commitment_sha256: Some(
                classifier_commitment_sha256,
            ),
            attended_review: MtgoAttendedCompetitiveEntryReviewCommitmentsV1 {
                source_lifecycle_snapshot_commitment_sha256: "2".repeat(64),
                permission_review_commitment_sha256: "6".repeat(64),
                mode_authorization_commitment_sha256: "7".repeat(64),
                request_commitment_sha256: "8".repeat(64),
                entry_authorization_sha256: "9".repeat(64),
                owner_review_receipt_sha256: "a".repeat(64),
                event_kind: MtgoCompetitiveEventKindV1::League,
                frame_id: 17,
                frame_sequence: 41,
                resource: MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                amount: 100,
            },
        }
    }

    fn classifier_bound_entry_review_commitments_v4(
        classifier_commitment_sha256: String,
    ) -> MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3 {
        let source_bound =
            source_bound_entry_review_commitments_v3(classifier_commitment_sha256.clone());
        let classifier_bound_review_commitment_sha256 =
            classifier_bound_competitive_entry_review_commitment_v3(
                &source_bound,
                &classifier_commitment_sha256,
            );
        MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3 {
            source_navigation_classification_result_commitment_sha256: classifier_commitment_sha256,
            classifier_bound_review_commitment_sha256,
            source_bound_review: source_bound,
        }
    }

    fn entry_control_dry_run_commitments_v4(
        classifier_commitment_sha256: String,
    ) -> MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1 {
        MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1 {
            source_identity_commitment_sha256: "5".repeat(64),
            source_capture_commitment_sha256: "1".repeat(64),
            source_lifecycle_snapshot_commitment_sha256: "2".repeat(64),
            source_navigation_classification_result_commitment_sha256: classifier_commitment_sha256,
            visible_control_label_sha256: "c".repeat(64),
            visible_control_region_sha256: "d".repeat(64),
            visibly_enabled_confirmed: true,
            dry_run_commitment_sha256: "e".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            frame_id: 17,
            frame_sequence: 41,
            resource: MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
            amount: 100,
        }
    }

    fn competitive_entry_ratification_parts_v1(
        event_kind: MtgoCompetitiveEventKindV1,
        resource: MtgoCompetitiveEntryResourceV1,
        amount: u32,
    ) -> (
        CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
        CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
        MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1,
        MtgoCompetitiveEntryAuthorizationV1,
        MtgoControlBoundCompetitiveEntryReviewCommitmentsV4,
    ) {
        let correspondence = checked_competitive_correspondence_v2();
        let source = checked_competitive_entry_review_snapshot_v1(event_kind, resource, amount);
        let reviewed_source =
            checked_competitive_entry_review_snapshot_v1(event_kind, resource, amount);
        let nonce = [0x31u8; 8];
        let phrase = attended_competitive_entry_review_confirmation_phrase_v1(
            event_kind, resource, amount, &nonce,
        );
        let reviewed = review_competitive_entry_from_attended_confirmation_v1(
            &correspondence,
            reviewed_source,
            "UnbuckledPie",
            match event_kind {
                MtgoCompetitiveEventKindV1::League => "Modern League".to_owned(),
                MtgoCompetitiveEventKindV1::Challenge => "Modern Challenge".to_owned(),
            },
            nonce,
            2_100,
            &phrase,
        )
        .unwrap();
        let attended = reviewed.commitments_v1();
        let entry_authorization = reviewed.entry_authorization_record_v1();
        let classifier_commitment = "b".repeat(64);
        let source_identity = MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1 {
            source_capture_commitment_sha256: "1".repeat(64),
            source_lifecycle_snapshot_commitment_sha256: source
                .snapshot_commitment_sha256()
                .to_owned(),
            source_window_title_sha256: "3".repeat(64),
            event_label_region_sha256: "4".repeat(64),
            source_identity_commitment_sha256: "5".repeat(64),
            source_navigation_classification_result_commitment_sha256: Some(
                classifier_commitment.clone(),
            ),
            event_kind,
            frame_id: source.frame_id_v1(),
            frame_sequence: source.frame_sequence(),
            resource,
            amount,
        };
        let source_bound = MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2 {
            source_identity_commitment_sha256: source_identity
                .source_identity_commitment_sha256
                .clone(),
            source_capture_commitment_sha256: source_identity
                .source_capture_commitment_sha256
                .clone(),
            source_navigation_classification_result_commitment_sha256: Some(
                classifier_commitment.clone(),
            ),
            attended_review: attended,
        };
        let classifier_bound = MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3 {
            source_navigation_classification_result_commitment_sha256: classifier_commitment
                .clone(),
            classifier_bound_review_commitment_sha256:
                classifier_bound_competitive_entry_review_commitment_v3(
                    &source_bound,
                    &classifier_commitment,
                ),
            source_bound_review: source_bound,
        };
        let dry_run = MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1 {
            source_identity_commitment_sha256: source_identity
                .source_identity_commitment_sha256
                .clone(),
            source_capture_commitment_sha256: source_identity
                .source_capture_commitment_sha256
                .clone(),
            source_lifecycle_snapshot_commitment_sha256: source_identity
                .source_lifecycle_snapshot_commitment_sha256
                .clone(),
            source_navigation_classification_result_commitment_sha256: classifier_commitment,
            visible_control_label_sha256: "c".repeat(64),
            visible_control_region_sha256: "d".repeat(64),
            visibly_enabled_confirmed: true,
            dry_run_commitment_sha256: "e".repeat(64),
            event_kind,
            frame_id: source.frame_id_v1(),
            frame_sequence: source.frame_sequence(),
            resource,
            amount,
        };
        let control_bound_review_commitment_sha256 =
            bind_control_bound_competitive_entry_review_commitment_v4(&classifier_bound, &dry_run)
                .unwrap();
        let control_bound = MtgoControlBoundCompetitiveEntryReviewCommitmentsV4 {
            control_bound_review_commitment_sha256,
            classifier_bound_review: classifier_bound,
            entry_control_dry_run: dry_run,
        };
        (
            correspondence,
            source,
            source_identity,
            entry_authorization,
            control_bound,
        )
    }

    fn competitive_entry_immediate_recapture_commitments_v1(
        candidate: &MtgoReviewedCompetitiveEntryRatificationCandidateV1,
    ) -> MtgoCompetitiveEntryImmediateRecaptureCommitmentsV1 {
        MtgoCompetitiveEntryImmediateRecaptureCommitmentsV1 {
            navigation_profile_commitment_sha256: "6".repeat(64),
            navigation_profile_admission_commitment_sha256: "7".repeat(64),
            approved_account_alias_sha256: candidate.account_alias_sha256.clone(),
            runtime_identity_commitment_sha256: "8".repeat(64),
            window_continuity_commitment_sha256: "9".repeat(64),
            source_identity_commitment_sha256: candidate.source_identity_commitment_sha256.clone(),
            source_capture_commitment_sha256: candidate.source_capture_commitment_sha256.clone(),
            source_classification_result_commitment_sha256: candidate
                .source_navigation_classification_result_commitment_sha256
                .clone(),
            source_lifecycle_snapshot_commitment_sha256: "a".repeat(64),
            immediate_capture_commitment_sha256: "b".repeat(64),
            immediate_classification_result_commitment_sha256: "c".repeat(64),
            immediate_lifecycle_snapshot_commitment_sha256: "d".repeat(64),
            event_label_region_sha256: "e".repeat(64),
            visible_control_label_sha256: "f".repeat(64),
            visible_control_region_sha256: candidate.visible_control_region_sha256.clone(),
            event_identity_sha256: candidate.event_identity_sha256.clone(),
            entry_terms_sha256: candidate.entry_terms_sha256.clone(),
            event_kind: candidate.event_kind,
            resource: candidate.resource,
            amount: candidate.amount,
            source_frame_id: 17,
            source_frame_sequence: 41,
            source_captured_at_unix_millis: 410,
            immediate_frame_id: 18,
            immediate_frame_sequence: 42,
            immediate_captured_at_unix_millis: 420,
            recapture_commitment_sha256: "0".repeat(64),
        }
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
    fn reviewed_gesture_permission_binds_all_families_profile_and_one_mode() {
        let families = canonical_duel_gesture_action_families_v1();
        let facts = CompetitiveDuelGestureProfileFactsV1 {
            evaluation_commitment_sha256: &"1".repeat(64),
            admission_commitment_sha256: &"2".repeat(64),
            runtime_binary_sha256: &"3".repeat(64),
            assets_manifest_sha256: &"4".repeat(64),
            supported_action_families: &families,
        };
        let mut commitments = Vec::new();
        for event_kind in [
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ] {
            let candidate = competitive_duel_gesture_ratification_candidate_from_parts_v1(
                &checked_competitive_correspondence_v2(),
                "UnbuckledPie",
                event_kind,
                &facts,
            )
            .unwrap();
            assert_eq!(candidate.event_kind, event_kind);
            assert_eq!(candidate.supported_action_families, families);
            assert_eq!(
                candidate.gesture_evaluation_commitment_sha256,
                facts.evaluation_commitment_sha256
            );
            assert_eq!(
                candidate.gesture_profile_admission_commitment_sha256,
                facts.admission_commitment_sha256
            );
            assert!(!candidate.safe_for_live_input_v1());
            assert!(!candidate.permits_event_entry_v1());
            assert!(!candidate.permits_spending_v1());
            commitments.push(candidate.ratification_commitment_sha256);
        }
        assert_ne!(commitments[0], commitments[1]);
        assert!(
            RATIFIED_COMPETITIVE_DUEL_GESTURE_AUTHORIZATION_FROM_REVIEW_COMMITMENT_V1.is_none()
        );

        let incomplete = &families[..families.len() - 1];
        let incomplete_facts = CompetitiveDuelGestureProfileFactsV1 {
            supported_action_families: incomplete,
            ..facts
        };
        assert!(
            competitive_duel_gesture_ratification_candidate_from_parts_v1(
                &checked_competitive_correspondence_v2(),
                "UnbuckledPie",
                MtgoCompetitiveEventKindV1::League,
                &incomplete_facts,
            )
            .is_err()
        );
        let duplicate_hash_facts = CompetitiveDuelGestureProfileFactsV1 {
            admission_commitment_sha256: facts.evaluation_commitment_sha256,
            ..facts
        };
        assert!(
            competitive_duel_gesture_ratification_candidate_from_parts_v1(
                &checked_competitive_correspondence_v2(),
                "UnbuckledPie",
                MtgoCompetitiveEventKindV1::League,
                &duplicate_hash_facts,
            )
            .is_err()
        );
        assert!(
            competitive_duel_gesture_ratification_candidate_from_parts_v1(
                &checked_competitive_correspondence_v2(),
                "DifferentAccount",
                MtgoCompetitiveEventKindV1::League,
                &facts,
            )
            .is_err()
        );
    }

    #[test]
    fn attended_gesture_launch_extension_binds_exact_pass_game_and_profile() {
        let families = canonical_duel_gesture_action_families_v1();
        let evaluation = "1".repeat(64);
        let admission = "2".repeat(64);
        let runtime = "3".repeat(64);
        let assets = "4".repeat(64);
        let facts = CompetitiveDuelGestureProfileFactsV1 {
            evaluation_commitment_sha256: &evaluation,
            admission_commitment_sha256: &admission,
            runtime_binary_sha256: &runtime,
            assets_manifest_sha256: &assets,
            supported_action_families: &families,
        };
        let correspondence = checked_competitive_correspondence_v2();
        let candidate = competitive_duel_gesture_ratification_candidate_from_parts_v1(
            &correspondence,
            "UnbuckledPie",
            MtgoCompetitiveEventKindV1::League,
            &facts,
        )
        .unwrap();
        let mut authorization =
            competitive_match_authorization_v1(MtgoCompetitiveEventKindV1::League, 2);
        authorization.account_alias_sha256 = candidate.account_alias_sha256.clone();
        authorization.written_permission_sha256 = candidate.correspondence_sha256.clone();
        let gameplay_authorization_commitment_sha256 =
            competitive_match_gameplay_authorization_commitment_v1(&authorization).unwrap();
        let launch_authorization_commitment_sha256 = competitive_match_launch_commitment_v1(
            "UnbuckledPie",
            &candidate.mode_authorization_commitment_sha256,
            &gameplay_authorization_commitment_sha256,
            &authorization,
        );
        let pass_match_launch = RatifiedMtgoCompetitiveMatchLaunchV1 {
            authorization,
            mode_authorization_commitment_sha256: candidate
                .mode_authorization_commitment_sha256
                .clone(),
            gameplay_authorization_commitment_sha256,
            launch_authorization_commitment_sha256,
            valid_from_frame_sequence: 40,
        };
        let nonce = [0x2au8; 8];
        let phrase = attended_competitive_gesture_match_launch_confirmation_phrase_v1(
            MtgoCompetitiveEventKindV1::League,
            2,
            &nonce,
        );
        assert_eq!(
            phrase,
            "AUTHORIZE MTGO LEAGUE GAME 2 ALL ELEVEN GESTURES 2A2A2A2A2A2A2A2A"
        );
        let extension = competitive_gesture_match_launch_commitment_from_facts_v1(
            &candidate,
            &pass_match_launch,
            "UnbuckledPie",
            &nonce,
            1_777,
            &phrase,
        )
        .unwrap();
        assert_eq!(extension.len(), 64);
        assert!(competitive_gesture_match_launch_commitment_from_facts_v1(
            &candidate,
            &pass_match_launch,
            "UnbuckledPie",
            &nonce,
            1_777,
            "AUTHORIZE SOMETHING ELSE",
        )
        .is_err());
        let wrong_game_phrase = attended_competitive_gesture_match_launch_confirmation_phrase_v1(
            MtgoCompetitiveEventKindV1::League,
            1,
            &nonce,
        );
        assert!(competitive_gesture_match_launch_commitment_from_facts_v1(
            &candidate,
            &pass_match_launch,
            "UnbuckledPie",
            &nonce,
            1_777,
            &wrong_game_phrase,
        )
        .is_err());
        assert!(competitive_gesture_match_launch_commitment_from_facts_v1(
            &candidate,
            &pass_match_launch,
            "UnbuckledPie",
            &nonce,
            0,
            &phrase,
        )
        .is_err());
        let session = initial_competitive_gesture_game_session_commitment_v1(
            &candidate,
            &pass_match_launch,
            &extension,
        );
        assert_eq!(session.len(), 64);
        assert_ne!(session, extension);

        let mut wrong_mode = candidate.clone();
        wrong_mode.event_kind = MtgoCompetitiveEventKindV1::Challenge;
        assert!(validate_competitive_gesture_match_launch_facts_v1(
            &wrong_mode,
            &pass_match_launch,
            "UnbuckledPie"
        )
        .is_err());
        let mut incomplete = candidate.clone();
        incomplete.supported_action_families.pop();
        assert!(validate_competitive_gesture_match_launch_facts_v1(
            &incomplete,
            &pass_match_launch,
            "UnbuckledPie"
        )
        .is_err());
        assert!(validate_competitive_gesture_match_launch_facts_v1(
            &candidate,
            &pass_match_launch,
            "DifferentAccount"
        )
        .is_err());
        let mut invalid_lifetime = pass_match_launch;
        invalid_lifetime.valid_from_frame_sequence = 0;
        assert!(validate_competitive_gesture_match_launch_facts_v1(
            &candidate,
            &invalid_lifetime,
            "UnbuckledPie"
        )
        .is_err());
    }

    #[test]
    fn all_family_session_binds_only_matching_source_gesture_sequence() {
        let session = MtgoCompetitiveGestureGameSessionCommitmentsV1 {
            session_commitment_sha256: "1".repeat(64),
            general_gesture_permission_commitment_sha256: "2".repeat(64),
            mode_authorization_commitment_sha256: "3".repeat(64),
            pass_match_launch_commitment_sha256: "4".repeat(64),
            match_gameplay_authorization_commitment_sha256: "5".repeat(64),
            gesture_match_launch_commitment_sha256: "6".repeat(64),
            gesture_evaluation_commitment_sha256: "7".repeat(64),
            gesture_profile_admission_commitment_sha256: "8".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 2,
            valid_from_frame_sequence: 40,
            valid_through_frame_sequence: 100,
            last_confirmed_frame_sequence: 39,
            confirmed_action_count: 0,
        };
        let sequence = MtgoOpaqueCompetitiveDuelGestureSequenceCommitmentsV1 {
            competitive_action_plan_commitment_sha256: "9".repeat(64),
            gesture_plan_commitment_sha256: "a".repeat(64),
            competitive_mode_authorization_commitment_sha256: session
                .mode_authorization_commitment_sha256
                .clone(),
            competitive_match_gameplay_authorization_commitment_sha256: session
                .match_gameplay_authorization_commitment_sha256
                .clone(),
            current_stage_binding_commitment_sha256: "b".repeat(64),
            current_opaque_stage_commitment_sha256: "c".repeat(64),
            last_visible_transition_commitment_sha256: None,
            sequence_commitment_sha256: "d".repeat(64),
            selected_action_family: MtgoDuelActionFamilyV1::PlayLand,
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 2,
            gameplay_authorization_valid_through_frame_sequence: 100,
            current_stage_index: 0,
            gesture_stage_count: 1,
            observed_stage_count: 1,
            current_frame_id: 12,
            current_frame_sequence: 40,
        };
        let bound =
            competitive_duel_gesture_sequence_session_binding_from_parts_v1(&sequence, &session)
                .unwrap();
        assert_eq!(
            bound.selected_action_family,
            MtgoDuelActionFamilyV1::PlayLand
        );
        assert_eq!(bound.event_kind, MtgoCompetitiveEventKindV1::League);
        assert_eq!(bound.game_number, 2);
        assert_eq!(bound.source_frame_sequence, 40);
        assert_eq!(bound.gesture_stage_count, 1);
        assert_eq!(bound.binding_commitment_sha256.len(), 64);

        let mut wrong_mode = sequence.clone();
        wrong_mode.competitive_mode_authorization_commitment_sha256 = "e".repeat(64);
        assert!(
            competitive_duel_gesture_sequence_session_binding_from_parts_v1(&wrong_mode, &session)
                .is_err()
        );
        let mut wrong_game = sequence.clone();
        wrong_game.game_number = 1;
        assert!(
            competitive_duel_gesture_sequence_session_binding_from_parts_v1(&wrong_game, &session)
                .is_err()
        );
        let mut stale = sequence.clone();
        stale.current_frame_sequence = 39;
        assert!(
            competitive_duel_gesture_sequence_session_binding_from_parts_v1(&stale, &session)
                .is_err()
        );
        let mut continuation = sequence.clone();
        continuation.current_stage_index = 1;
        continuation.observed_stage_count = 2;
        continuation.last_visible_transition_commitment_sha256 = Some("e".repeat(64));
        assert!(
            competitive_duel_gesture_sequence_session_binding_from_parts_v1(
                &continuation,
                &session
            )
            .is_err()
        );
        let mut wrong_lifetime = sequence;
        wrong_lifetime.gameplay_authorization_valid_through_frame_sequence = 99;
        assert!(
            competitive_duel_gesture_sequence_session_binding_from_parts_v1(
                &wrong_lifetime,
                &session
            )
            .is_err()
        );
    }

    #[test]
    fn session_bound_gesture_preparation_requires_exact_next_frame_and_authority() {
        let session = MtgoCompetitiveGestureGameSessionCommitmentsV1 {
            session_commitment_sha256: "1".repeat(64),
            general_gesture_permission_commitment_sha256: "2".repeat(64),
            mode_authorization_commitment_sha256: "3".repeat(64),
            pass_match_launch_commitment_sha256: "4".repeat(64),
            match_gameplay_authorization_commitment_sha256: "5".repeat(64),
            gesture_match_launch_commitment_sha256: "6".repeat(64),
            gesture_evaluation_commitment_sha256: "7".repeat(64),
            gesture_profile_admission_commitment_sha256: "8".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::Challenge,
            game_number: 1,
            valid_from_frame_sequence: 90,
            valid_through_frame_sequence: 120,
            last_confirmed_frame_sequence: 89,
            confirmed_action_count: 0,
        };
        let sequence = MtgoOpaqueCompetitiveDuelGestureSequenceCommitmentsV1 {
            competitive_action_plan_commitment_sha256: "9".repeat(64),
            gesture_plan_commitment_sha256: "a".repeat(64),
            competitive_mode_authorization_commitment_sha256: session
                .mode_authorization_commitment_sha256
                .clone(),
            competitive_match_gameplay_authorization_commitment_sha256: session
                .match_gameplay_authorization_commitment_sha256
                .clone(),
            current_stage_binding_commitment_sha256: "b".repeat(64),
            current_opaque_stage_commitment_sha256: "c".repeat(64),
            last_visible_transition_commitment_sha256: None,
            sequence_commitment_sha256: "d".repeat(64),
            selected_action_family: MtgoDuelActionFamilyV1::CastOrPlotSpell,
            event_kind: MtgoCompetitiveEventKindV1::Challenge,
            game_number: 1,
            gameplay_authorization_valid_through_frame_sequence: 120,
            current_stage_index: 0,
            gesture_stage_count: 2,
            observed_stage_count: 1,
            current_frame_id: 90,
            current_frame_sequence: 90,
        };
        let bound =
            competitive_duel_gesture_sequence_session_binding_from_parts_v1(&sequence, &session)
                .unwrap();
        let prepared = MtgoOpaqueCompetitiveDuelGestureSourcePreparationCommitmentsV1 {
            source_sequence_commitment_sha256: sequence.sequence_commitment_sha256.clone(),
            competitive_action_plan_commitment_sha256: sequence
                .competitive_action_plan_commitment_sha256
                .clone(),
            gesture_plan_commitment_sha256: sequence.gesture_plan_commitment_sha256.clone(),
            competitive_mode_authorization_commitment_sha256: session
                .mode_authorization_commitment_sha256
                .clone(),
            competitive_match_gameplay_authorization_commitment_sha256: session
                .match_gameplay_authorization_commitment_sha256
                .clone(),
            fresh_stage_binding_commitment_sha256: "e".repeat(64),
            fresh_capture_commitment_sha256: "f".repeat(64),
            fresh_perception_result_commitment_sha256: "0".repeat(64),
            gesture_target_runtime_identity_commitment_sha256: "3".repeat(64),
            gesture_target_request_commitment_sha256: "4".repeat(64),
            primitive_commitment_sha256: "1".repeat(64),
            preparation_commitment_sha256: "2".repeat(64),
            selected_action_family: MtgoDuelActionFamilyV1::CastOrPlotSpell,
            event_kind: MtgoCompetitiveEventKindV1::Challenge,
            game_number: 1,
            stage_index: 0,
            gesture_stage_count: 2,
            target_count: 1,
            fresh_frame_id: 91,
            fresh_frame_sequence: 91,
            fresh_captured_at_unix_millis: 1,
            gameplay_authorization_valid_through_frame_sequence: 120,
        };
        let checked =
            competitive_duel_gesture_source_preparation_from_parts_v1(&bound, &session, &prepared)
                .unwrap();
        assert_eq!(checked.fresh_frame_sequence, 91);
        assert_eq!(checked.stage_index, 0);
        assert_eq!(checked.target_count, 1);
        assert_eq!(checked.preparation_binding_commitment_sha256.len(), 64);

        let mut skipped = prepared.clone();
        skipped.fresh_frame_sequence = 92;
        assert!(competitive_duel_gesture_source_preparation_from_parts_v1(
            &bound, &session, &skipped
        )
        .is_err());
        let mut wrong_authority = prepared.clone();
        wrong_authority.competitive_match_gameplay_authorization_commitment_sha256 = "a".repeat(64);
        assert!(competitive_duel_gesture_source_preparation_from_parts_v1(
            &bound,
            &session,
            &wrong_authority
        )
        .is_err());
        let mut continuation = prepared.clone();
        continuation.stage_index = 1;
        assert!(competitive_duel_gesture_source_preparation_from_parts_v1(
            &bound,
            &session,
            &continuation
        )
        .is_err());
        let mut too_many_targets = prepared;
        too_many_targets.target_count = 3;
        assert!(competitive_duel_gesture_source_preparation_from_parts_v1(
            &bound,
            &session,
            &too_many_targets
        )
        .is_err());
    }

    #[test]
    fn attended_entry_review_binds_both_approved_modes_and_exact_visible_terms() {
        for (event_kind, label, resource, amount) in [
            (
                MtgoCompetitiveEventKindV1::League,
                "Modern League",
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            ),
            (
                MtgoCompetitiveEventKindV1::Challenge,
                "Modern Challenge",
                MtgoCompetitiveEntryResourceV1::ExistingEventTickets,
                25,
            ),
        ] {
            let correspondence = checked_competitive_correspondence_v2();
            let source = checked_competitive_entry_review_snapshot_v1(event_kind, resource, amount);
            let nonce = [0x5au8; 8];
            let phrase = attended_competitive_entry_review_confirmation_phrase_v1(
                event_kind, resource, amount, &nonce,
            );
            let reviewed = review_competitive_entry_from_attended_confirmation_v1(
                &correspondence,
                source,
                "UnbuckledPie",
                label.to_owned(),
                nonce,
                2_000,
                &phrase,
            )
            .unwrap();
            let commitments = reviewed.commitments_v1();
            let entry = reviewed.entry_authorization_record_v1();
            assert_eq!(commitments.event_kind, event_kind);
            assert_eq!(commitments.resource, resource);
            assert_eq!(commitments.amount, amount);
            assert_eq!(commitments.frame_id, 17);
            assert_eq!(commitments.frame_sequence, 41);
            assert_eq!(entry.event_kind, event_kind);
            assert_eq!(entry.entry_terms.resource, resource);
            assert_eq!(entry.entry_terms.amount, amount);
            assert!(entry.exact_entry_authorized);
            assert!(entry.existing_account_resources_only);

            let expected_source =
                checked_competitive_entry_review_snapshot_v1(event_kind, resource, amount);
            let scope = correspondence
                .checked_untrusted_scope_for_mode_v1(event_kind)
                .unwrap();
            let expected_intent = make_offline_competitive_lifecycle_intent_v1(
                &expected_source,
                MtgoCompetitiveLifecycleActionV1::ConfirmEntry,
                &scope,
                Some(&entry),
            )
            .unwrap();
            assert_eq!(
                expected_intent.entry_authorization_sha256.as_deref(),
                Some(commitments.entry_authorization_sha256.as_str())
            );
            assert_eq!(
                commitments.permission_review_commitment_sha256,
                correspondence.review_commitment_sha256()
            );
            assert!(!reviewed.safe_for_live_input_v1());
            assert!(!reviewed.permits_event_entry_v1());
            assert!(!reviewed.permits_spending_v1());
        }
    }

    #[test]
    fn classifier_bound_entry_review_requires_and_binds_exact_classifier_lineage() {
        let missing = entry_source_identity_commitments_v3(None);
        assert!(require_classifier_bound_competitive_entry_source_v3(&missing).is_err());

        let malformed =
            entry_source_identity_commitments_v3(Some("not-a-classifier-digest".to_owned()));
        assert!(require_classifier_bound_competitive_entry_source_v3(&malformed).is_err());

        let classifier = "b".repeat(64);
        let exact = entry_source_identity_commitments_v3(Some(classifier.clone()));
        assert_eq!(
            require_classifier_bound_competitive_entry_source_v3(&exact).unwrap(),
            classifier
        );

        let source_bound = source_bound_entry_review_commitments_v3(classifier.clone());
        let baseline =
            classifier_bound_competitive_entry_review_commitment_v3(&source_bound, &classifier);
        assert_eq!(baseline.len(), 64);

        let different_classifier = "c".repeat(64);
        let changed_classifier =
            source_bound_entry_review_commitments_v3(different_classifier.clone());
        assert_ne!(
            baseline,
            classifier_bound_competitive_entry_review_commitment_v3(
                &changed_classifier,
                &different_classifier,
            )
        );

        let mut changed_terms = source_bound;
        changed_terms.attended_review.amount = 120;
        assert_ne!(
            baseline,
            classifier_bound_competitive_entry_review_commitment_v3(&changed_terms, &classifier,)
        );
    }

    #[test]
    fn control_bound_attended_review_requires_the_exact_enabled_dry_run() {
        let classifier = "b".repeat(64);
        let classifier_bound = classifier_bound_entry_review_commitments_v4(classifier.clone());
        let dry_run = entry_control_dry_run_commitments_v4(classifier);
        let baseline =
            bind_control_bound_competitive_entry_review_commitment_v4(&classifier_bound, &dry_run)
                .unwrap();
        assert_eq!(baseline.len(), 64);

        let mut disabled = dry_run.clone();
        disabled.visibly_enabled_confirmed = false;
        assert!(bind_control_bound_competitive_entry_review_commitment_v4(
            &classifier_bound,
            &disabled,
        )
        .is_err());

        let mut wrong_frame = dry_run.clone();
        wrong_frame.frame_sequence += 1;
        assert!(bind_control_bound_competitive_entry_review_commitment_v4(
            &classifier_bound,
            &wrong_frame,
        )
        .is_err());

        let mut wrong_classifier = dry_run.clone();
        wrong_classifier.source_navigation_classification_result_commitment_sha256 = "f".repeat(64);
        assert!(bind_control_bound_competitive_entry_review_commitment_v4(
            &classifier_bound,
            &wrong_classifier,
        )
        .is_err());

        let mut changed_control = dry_run;
        changed_control.visible_control_region_sha256 = "0".repeat(64);
        let changed = bind_control_bound_competitive_entry_review_commitment_v4(
            &classifier_bound,
            &changed_control,
        )
        .unwrap();
        assert_ne!(baseline, changed);
    }

    #[test]
    fn competitive_entry_ratification_candidate_binds_exact_mode_terms_and_review() {
        let mut commitments = Vec::new();
        for (event_kind, resource, amount) in [
            (
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            ),
            (
                MtgoCompetitiveEventKindV1::Challenge,
                MtgoCompetitiveEntryResourceV1::ExistingEventTickets,
                25,
            ),
        ] {
            let (correspondence, source, source_identity, entry_authorization, review) =
                competitive_entry_ratification_parts_v1(event_kind, resource, amount);
            let candidate = competitive_entry_ratification_candidate_from_parts_v1(
                &correspondence,
                "UnbuckledPie",
                &source,
                &source_identity,
                &entry_authorization,
                &review,
            )
            .unwrap();
            assert_eq!(candidate.event_kind, event_kind);
            assert_eq!(candidate.resource, resource);
            assert_eq!(candidate.amount, amount);
            assert_eq!(candidate.ratification_commitment_sha256.len(), 64);
            assert_eq!(
                candidate.permission_review_commitment_sha256,
                correspondence.review_commitment_sha256()
            );
            assert!(!candidate.safe_for_live_input_v1());
            assert!(!candidate.permits_event_entry_v1());
            assert!(!candidate.permits_spending_v1());
            commitments.push(candidate.ratification_commitment_sha256);
        }
        assert_ne!(commitments[0], commitments[1]);
        assert!(RATIFIED_COMPETITIVE_ENTRY_AUTHORIZATION_COMMITMENT_V1.is_none());
    }

    #[test]
    fn competitive_entry_ratification_rejects_lineage_terms_and_control_drift() {
        let (correspondence, source, source_identity, entry_authorization, review) =
            competitive_entry_ratification_parts_v1(
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            );
        let check =
            |identity: &MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1,
             authorization: &MtgoCompetitiveEntryAuthorizationV1,
             candidate_review: &MtgoControlBoundCompetitiveEntryReviewCommitmentsV4| {
                competitive_entry_ratification_candidate_from_parts_v1(
                    &correspondence,
                    "UnbuckledPie",
                    &source,
                    identity,
                    authorization,
                    candidate_review,
                )
            };

        let mut wrong_account = entry_authorization.clone();
        wrong_account.account_alias_sha256 = "0".repeat(64);
        assert!(check(&source_identity, &wrong_account, &review).is_err());

        let mut wrong_terms = entry_authorization.clone();
        wrong_terms.entry_terms.amount = 120;
        assert!(check(&source_identity, &wrong_terms, &review).is_err());

        let mut wrong_control = review.clone();
        wrong_control
            .entry_control_dry_run
            .visible_control_region_sha256 = "0".repeat(64);
        assert!(check(&source_identity, &entry_authorization, &wrong_control).is_err());

        let mut wrong_classifier = source_identity.clone();
        wrong_classifier.source_navigation_classification_result_commitment_sha256 =
            Some("0".repeat(64));
        assert!(check(&wrong_classifier, &entry_authorization, &review).is_err());

        assert!(competitive_entry_ratification_candidate_from_parts_v1(
            &correspondence,
            "DifferentAccount",
            &source,
            &source_identity,
            &entry_authorization,
            &review,
        )
        .is_err());
    }

    #[test]
    fn competitive_entry_preparation_binds_fresh_league_and_challenge_recaptures() {
        let mut commitments = Vec::new();
        for (event_kind, resource, amount) in [
            (
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            ),
            (
                MtgoCompetitiveEventKindV1::Challenge,
                MtgoCompetitiveEntryResourceV1::ExistingEventTickets,
                25,
            ),
        ] {
            let (correspondence, source, source_identity, entry_authorization, review) =
                competitive_entry_ratification_parts_v1(event_kind, resource, amount);
            let candidate = competitive_entry_ratification_candidate_from_parts_v1(
                &correspondence,
                "UnbuckledPie",
                &source,
                &source_identity,
                &entry_authorization,
                &review,
            )
            .unwrap();
            let recapture = competitive_entry_immediate_recapture_commitments_v1(&candidate);
            let prepared =
                competitive_entry_preparation_from_commitments_v1(&candidate, &recapture).unwrap();
            assert_eq!(
                prepared.entry_ratification_commitment_sha256,
                candidate.ratification_commitment_sha256
            );
            assert_eq!(prepared.event_kind, event_kind);
            assert_eq!(prepared.resource, resource);
            assert_eq!(prepared.amount, amount);
            assert_eq!(prepared.immediate_frame_id, 18);
            assert_eq!(prepared.immediate_frame_sequence, 42);
            assert_eq!(prepared.preparation_commitment_sha256.len(), 64);
            commitments.push(prepared.preparation_commitment_sha256);
        }
        assert_ne!(commitments[0], commitments[1]);
    }

    #[test]
    fn competitive_entry_preparation_rejects_ratification_and_recapture_drift() {
        let (correspondence, source, source_identity, entry_authorization, review) =
            competitive_entry_ratification_parts_v1(
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            );
        let candidate = competitive_entry_ratification_candidate_from_parts_v1(
            &correspondence,
            "UnbuckledPie",
            &source,
            &source_identity,
            &entry_authorization,
            &review,
        )
        .unwrap();
        let recapture = competitive_entry_immediate_recapture_commitments_v1(&candidate);

        let mut wrong_account = recapture.clone();
        wrong_account.approved_account_alias_sha256 = "1".repeat(64);
        assert!(
            competitive_entry_preparation_from_commitments_v1(&candidate, &wrong_account).is_err()
        );

        let mut wrong_control = recapture.clone();
        wrong_control.visible_control_region_sha256 = "1".repeat(64);
        assert!(
            competitive_entry_preparation_from_commitments_v1(&candidate, &wrong_control).is_err()
        );

        let mut wrong_terms = recapture.clone();
        wrong_terms.amount += 1;
        assert!(
            competitive_entry_preparation_from_commitments_v1(&candidate, &wrong_terms).is_err()
        );

        let mut stale = recapture.clone();
        stale.immediate_frame_sequence = stale.source_frame_sequence;
        assert!(competitive_entry_preparation_from_commitments_v1(&candidate, &stale).is_err());

        let mut invalid_runtime = recapture;
        invalid_runtime.runtime_identity_commitment_sha256 = "not-a-digest".to_owned();
        assert!(
            competitive_entry_preparation_from_commitments_v1(&candidate, &invalid_runtime)
                .is_err()
        );
    }

    #[test]
    fn competitive_entry_input_receipt_binds_preparation_target_time_and_cursor_result() {
        let (correspondence, source, source_identity, entry_authorization, review) =
            competitive_entry_ratification_parts_v1(
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            );
        let candidate = competitive_entry_ratification_candidate_from_parts_v1(
            &correspondence,
            "UnbuckledPie",
            &source,
            &source_identity,
            &entry_authorization,
            &review,
        )
        .unwrap();
        let recapture = competitive_entry_immediate_recapture_commitments_v1(&candidate);
        let prepared =
            competitive_entry_preparation_from_commitments_v1(&candidate, &recapture).unwrap();
        let target = MtgoCompetitiveEntryPointerTargetV1 {
            hwnd: 17,
            process_id: 23,
            process_start_filetime_100ns: 31,
            dpi: 120,
            client_rect_desktop_px: crate::SignedRectV1 {
                left: -100,
                top: 20,
                right: 1140,
                bottom: 760,
            },
            target_x_desktop_px: 900,
            target_y_desktop_px: 600,
            park_x_desktop_px: 1200,
            park_y_desktop_px: 700,
        };
        let baseline = competitive_entry_input_receipt_from_parts_v1(&prepared, &target, 500, true);
        assert_eq!(baseline.len(), 64);

        let mut changed_target = MtgoCompetitiveEntryPointerTargetV1 { ..target.clone() };
        changed_target.target_x_desktop_px += 1;
        assert_ne!(
            baseline,
            competitive_entry_input_receipt_from_parts_v1(&prepared, &changed_target, 500, true)
        );
        assert_ne!(
            baseline,
            competitive_entry_input_receipt_from_parts_v1(&prepared, &changed_target, 501, true)
        );
        assert_ne!(
            baseline,
            competitive_entry_input_receipt_from_parts_v1(&prepared, &target, 500, false)
        );
    }

    #[test]
    fn competitive_entry_confirmation_receipt_binds_visible_result_and_candidate_count() {
        let (correspondence, source, source_identity, entry_authorization, review) =
            competitive_entry_ratification_parts_v1(
                MtgoCompetitiveEventKindV1::Challenge,
                MtgoCompetitiveEntryResourceV1::ExistingEventTickets,
                25,
            );
        let candidate = competitive_entry_ratification_candidate_from_parts_v1(
            &correspondence,
            "UnbuckledPie",
            &source,
            &source_identity,
            &entry_authorization,
            &review,
        )
        .unwrap();
        let recapture = competitive_entry_immediate_recapture_commitments_v1(&candidate);
        let prepared =
            competitive_entry_preparation_from_commitments_v1(&candidate, &recapture).unwrap();
        let input = MtgoCompetitiveEntryInputReceiptCommitmentsV1 {
            preparation_commitment_sha256: prepared.preparation_commitment_sha256.clone(),
            entry_ratification_commitment_sha256: prepared
                .entry_ratification_commitment_sha256
                .clone(),
            input_receipt_sha256: "1".repeat(64),
            event_kind: prepared.event_kind,
            resource: prepared.resource,
            amount: prepared.amount,
            immediate_frame_id: prepared.immediate_frame_id,
            immediate_frame_sequence: prepared.immediate_frame_sequence,
            input_sent_at_unix_millis: 500,
            cursor_parked_outside_client: true,
        };
        let transition = MtgoCompetitiveEntryFrameTransitionCommitmentsV1 {
            navigation_profile_commitment_sha256: recapture.navigation_profile_commitment_sha256,
            navigation_profile_admission_commitment_sha256: recapture
                .navigation_profile_admission_commitment_sha256,
            approved_account_alias_sha256: recapture.approved_account_alias_sha256,
            runtime_identity_commitment_sha256: recapture.runtime_identity_commitment_sha256,
            window_continuity_commitment_sha256: recapture.window_continuity_commitment_sha256,
            source_identity_commitment_sha256: recapture.source_identity_commitment_sha256,
            before_capture_commitment_sha256: recapture.immediate_capture_commitment_sha256,
            before_classification_result_commitment_sha256: recapture
                .immediate_classification_result_commitment_sha256,
            before_lifecycle_snapshot_commitment_sha256: recapture
                .immediate_lifecycle_snapshot_commitment_sha256,
            after_capture_commitment_sha256: "2".repeat(64),
            after_classification_result_commitment_sha256: "3".repeat(64),
            after_lifecycle_snapshot_commitment_sha256: "4".repeat(64),
            event_kind: prepared.event_kind,
            event_identity_sha256: recapture.event_identity_sha256,
            before_frame_id: prepared.immediate_frame_id,
            before_frame_sequence: prepared.immediate_frame_sequence,
            after_frame_id: 19,
            after_frame_sequence: prepared.immediate_frame_sequence + 1,
            transition_commitment_sha256: "5".repeat(64),
        };
        let visible = MtgoCompetitiveEntryVisibleConfirmationCommitmentsV1 {
            frame_transition: transition,
            after_capture_commitment_sha256: "2".repeat(64),
            after_classification_result_commitment_sha256: "3".repeat(64),
            after_lifecycle_snapshot_commitment_sha256: "4".repeat(64),
            after_captured_at_unix_millis: 600,
            postcondition_candidate_count: 2,
            confirmation_commitment_sha256: "6".repeat(64),
        };
        let baseline = competitive_entry_confirmation_receipt_v1(&input, &prepared, &visible);
        assert_eq!(baseline.len(), 64);
        let mut changed_count = visible.clone();
        changed_count.postcondition_candidate_count += 1;
        assert_ne!(
            baseline,
            competitive_entry_confirmation_receipt_v1(&input, &prepared, &changed_count)
        );
        let mut changed_after = visible;
        changed_after.after_capture_commitment_sha256 = "7".repeat(64);
        assert_ne!(
            baseline,
            competitive_entry_confirmation_receipt_v1(&input, &prepared, &changed_after)
        );
    }

    #[test]
    fn attended_entry_review_receipt_changes_with_exact_resource_amount_and_label() {
        fn receipt(resource: MtgoCompetitiveEntryResourceV1, amount: u32, label: &str) -> String {
            let correspondence = checked_competitive_correspondence_v2();
            let source = checked_competitive_entry_review_snapshot_v1(
                MtgoCompetitiveEventKindV1::League,
                resource,
                amount,
            );
            let nonce = [0x41u8; 8];
            let phrase = attended_competitive_entry_review_confirmation_phrase_v1(
                MtgoCompetitiveEventKindV1::League,
                resource,
                amount,
                &nonce,
            );
            review_competitive_entry_from_attended_confirmation_v1(
                &correspondence,
                source,
                "UnbuckledPie",
                label.to_owned(),
                nonce,
                2_001,
                &phrase,
            )
            .unwrap()
            .commitments_v1()
            .owner_review_receipt_sha256
        }

        let baseline = receipt(
            MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
            100,
            "Modern League",
        );
        assert_ne!(
            baseline,
            receipt(
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                120,
                "Modern League"
            )
        );
        assert_ne!(
            baseline,
            receipt(
                MtgoCompetitiveEntryResourceV1::ExistingEventTickets,
                10,
                "Modern League"
            )
        );
        assert_ne!(
            baseline,
            receipt(
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
                "Vintage League"
            )
        );
    }

    #[test]
    fn attended_entry_review_rejects_alias_mode_source_or_challenge_drift() {
        let correspondence = checked_competitive_correspondence_v2();
        let source = checked_competitive_entry_review_snapshot_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
            100,
        );
        assert!(build_attended_competitive_entry_review_request_v1(
            &correspondence,
            &source,
            "DifferentAccount",
            "Modern League",
        )
        .is_err());
        assert!(build_attended_competitive_entry_review_request_v1(
            &correspondence,
            &source,
            "UnbuckledPie",
            "Modern Challenge",
        )
        .is_err());
        assert!(build_attended_competitive_entry_review_request_v1(
            &correspondence,
            &source,
            "UnbuckledPie",
            "Modern\nLeague",
        )
        .is_err());

        let mut request = build_attended_competitive_entry_review_request_v1(
            &correspondence,
            &source,
            "UnbuckledPie",
            "Modern League",
        )
        .unwrap();
        request.entry_terms.amount = 120;
        assert!(validate_attended_competitive_entry_review_request_v1(
            &correspondence,
            &source,
            "UnbuckledPie",
            &request,
        )
        .is_err());

        let nonce = [0x22u8; 8];
        assert!(review_competitive_entry_from_attended_confirmation_v1(
            &correspondence,
            checked_competitive_entry_review_snapshot_v1(
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            ),
            "UnbuckledPie",
            "Modern League".to_owned(),
            nonce,
            2_002,
            "AUTHORIZE SOMETHING ELSE",
        )
        .is_err());
        let phrase = attended_competitive_entry_review_confirmation_phrase_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
            100,
            &nonce,
        );
        assert!(review_competitive_entry_from_attended_confirmation_v1(
            &correspondence,
            checked_competitive_entry_review_snapshot_v1(
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            ),
            "UnbuckledPie",
            "Modern League".to_owned(),
            [0; 8],
            2_002,
            &phrase,
        )
        .is_err());
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
