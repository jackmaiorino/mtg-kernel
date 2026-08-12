use super::{
    capture_admitted_mtgo_duel_visible_frame_v1, choose_cursor_park_point_v3,
    competitive_entry_window_continuity_commitment_for_frame_v1,
    mtgo_process_continuity_commitment_for_frame_v1, serialize_manifest_v2, sha256_hex_v1,
    MtgoAdmittedDuelVisibleFrameCommitmentsV1, OpaqueMtgoAdmittedDuelVisibleFrameV1, SignedRectV1,
};
use mtgo_blackbox_v1::{
    bind_profile_bound_action_plan_to_competitive_match_v1, bind_visible_duel_gesture_stage_v1,
    canonical_duel_gesture_action_families_v1,
    check_untrusted_competitive_gameplay_before_input_pixels_v1,
    check_untrusted_competitive_gameplay_postcondition_pixels_v1,
    check_untrusted_dxgi_capture_artifact_v1, check_untrusted_dxgi_observed_decision_candidate_v1,
    duel_action_family_v1,
    inspect_untrusted_competitive_gameplay_postcondition_candidate_pixels_v1,
    prepare_profile_bound_action_postcondition_plan_v1, preview_output_identity_commitment_v1,
    recheck_visible_duel_gesture_source_stage_v1, required_duel_gesture_target_roles_v1,
    resolve_profile_bound_selected_visible_control_v1,
    score_and_select_profile_bound_duel_candidate_v1,
    validate_dxgi_bound_observation_reconstruction_audit_v1, validate_observed_decision_v1,
    validate_profile_bound_duel_gesture_plan_v1,
    validate_visible_competitive_lifecycle_snapshot_v1, visible_frame_region_content_sha256_v1,
    AdmittedMtgoCompetitiveDuelLifecycleProfileV1, AdmittedMtgoDuelGestureProfileV1,
    AdmittedMtgoDuelPerceptionProfileV1, CheckedUntrustedMtgoCompetitiveGameplayActionPlanV1,
    CheckedUntrustedMtgoCompetitiveGameplayBeforeInputV1,
    CheckedUntrustedMtgoCompetitiveGameplayPostconditionV1,
    CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1, CheckedUntrustedMtgoDuelGesturePlanV1,
    CheckedUntrustedMtgoDuelGestureStageBindingV1,
    CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1,
    CheckedUntrustedMtgoPlayerVisibleProfileBoundDuelModelSelectionV1,
    CheckedUntrustedMtgoPlayerVisibleResolvedActionControlV1,
    CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1,
    CheckedUntrustedMtgoProfileBoundResolvedActionControlV1,
    LoadedMtgoNativeCheckpointDeploymentV1, MtgoAuthorizationScopeV1, MtgoCompetitiveEventKindV1,
    MtgoCompetitiveLifecyclePhaseV1, MtgoCompetitiveMatchGameplayAuthorizationV1,
    MtgoDuelActionFamilyV1, MtgoDuelGesturePlanV1, MtgoDuelGesturePrimitiveV1,
    MtgoDuelGestureStageV1, MtgoDuelGestureTargetRoleV1, MtgoDxgiCaptureRoleV2,
    MtgoEvidenceSourceV1, MtgoExpectedModelDeploymentV1, MtgoLifecycleVisibleFactKindV1,
    MtgoNativeCheckpointObservationScorerV1, MtgoObservationReconstructionAuditV1,
    MtgoObservedDecisionV1, MtgoPlayerVisibleDuelActionV1, MtgoPlayerVisibleDuelScorerV1,
    MtgoProfileBoundPostconditionAfterFrameMetadataV1,
    MtgoProfileBoundPostconditionBeforeInputFrameV1, MtgoProfileBoundPostconditionCalibrationV1,
    MtgoProfileBoundPostconditionCandidateStatusV1, MtgoProfileBoundPostconditionRegionSetV1,
    MtgoRectPxV1, MtgoSignedRectDesktopPxV1, MtgoSizePxV1, MtgoVisibleActionControlSetV1,
    MtgoVisibleCompetitiveLifecycleSnapshotV1, MtgoVisibleDuelGestureTargetSetV1,
    ValidatedMtgoObservedDecisionV1, MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1,
    MTGO_DUEL_GESTURE_PLAN_SCHEMA_V1, MTGO_PROFILE_BOUND_POSTCONDITION_AFTER_FRAME_SCHEMA_V1,
    MTGO_PROFILE_BOUND_POSTCONDITION_BEFORE_INPUT_FRAME_SCHEMA_V1,
    MTGO_VISIBLE_ACTION_CONTROL_SET_SCHEMA_V1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const DUEL_PERCEPTION_RUNTIME_IDENTITY_DOMAIN_V1: &[u8] =
    b"mtgo-duel-perception-runtime-identity-v1";
const DUEL_PERCEPTION_REQUEST_DOMAIN_V1: &[u8] = b"mtgo-duel-perception-request-v1";
const DUEL_GESTURE_TARGET_RUNTIME_IDENTITY_DOMAIN_V1: &[u8] =
    b"mtgo-duel-gesture-target-runtime-identity-v1";
const DUEL_GESTURE_TARGET_REQUEST_DOMAIN_V1: &[u8] = b"mtgo-duel-gesture-target-request-v1";
const DUEL_PERCEPTION_RESULT_DOMAIN_V1: &[u8] = b"mtgo-duel-perception-result-v1";
#[allow(dead_code)]
const DUEL_OPAQUE_MODEL_SELECTION_DOMAIN_V1: &[u8] = b"mtgo-opaque-duel-model-selection-v1";
#[allow(dead_code)]
const DUEL_OPAQUE_CONTROL_RESOLUTION_DOMAIN_V1: &[u8] = b"mtgo-opaque-duel-control-resolution-v1";
const DUEL_OPAQUE_COMPETITIVE_LAUNCH_IDENTITY_DOMAIN_V1: &[u8] =
    b"mtgo-opaque-competitive-launch-visible-identity-v1";
const DUEL_OPAQUE_COMPETITIVE_ACTION_PLAN_DOMAIN_V1: &[u8] =
    b"mtgo-opaque-competitive-duel-action-plan-v1";
const DUEL_OPAQUE_COMPETITIVE_GESTURE_STAGE_DOMAIN_V1: &[u8] =
    b"mtgo-opaque-competitive-duel-gesture-stage-v1";
const DUEL_OPAQUE_COMPETITIVE_GESTURE_SEQUENCE_DOMAIN_V1: &[u8] =
    b"mtgo-opaque-competitive-duel-gesture-sequence-v1";
const DUEL_OPAQUE_COMPETITIVE_GESTURE_TRANSITION_DOMAIN_V1: &[u8] =
    b"mtgo-opaque-competitive-duel-gesture-transition-v1";
const DUEL_OPAQUE_COMPETITIVE_GESTURE_SOURCE_PREPARATION_DOMAIN_V1: &[u8] =
    b"mtgo-opaque-competitive-duel-gesture-source-preparation-v1";
const DUEL_OPAQUE_COMPETITIVE_GESTURE_CONFIRMATION_DOMAIN_V1: &[u8] =
    b"mtgo-opaque-competitive-duel-gesture-confirmation-v1";
const DUEL_OPAQUE_PINNED_COMPETITIVE_GESTURE_CONTINUATION_DOMAIN_V1: &[u8] =
    b"mtgo-opaque-pinned-competitive-duel-gesture-continuation-v1";
const DUEL_OPAQUE_COMPETITIVE_PASS_PREPARATION_DOMAIN_V1: &[u8] =
    b"mtgo-opaque-competitive-duel-pass-preparation-v1";
const DUEL_OPAQUE_COMPETITIVE_PASS_CONFIRMATION_DOMAIN_V1: &[u8] =
    b"mtgo-opaque-competitive-duel-pass-confirmation-v1";
const DUEL_PERCEPTION_PROTOCOL_MAGIC_V1: &[u8] = b"MTGO_VISIBLE_DUEL_PERCEPTION_V1\0";
const COMPETITIVE_PREGAME_PROTOCOL_MAGIC_V1: &[u8] = b"MTGO_VISIBLE_COMPETITIVE_PREGAME_V1\0";
const COMPETITIVE_PREGAME_PUBLIC_CONTEXT_PROTOCOL_MAGIC_V1: &[u8] =
    b"MTGO_VISIBLE_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_V1\0";
const DUEL_GESTURE_TARGET_PROTOCOL_MAGIC_V1: &[u8] = b"MTGO_VISIBLE_DUEL_GESTURE_TARGET_V1\0";
const MAX_RUNTIME_ARTIFACT_BYTES_V1: u64 = 512 * 1024 * 1024;
const MAX_PERCEPTION_RESPONSE_BYTES_V1: usize = 16 * 1024 * 1024;
const MAX_PERCEPTION_STDERR_BYTES_V1: usize = 64 * 1024;

pub const MTGO_OPAQUE_COMPETITIVE_DUEL_GESTURE_TRANSITION_SCHEMA_V1: u32 = 1;

/// Caller-assigned local identity for one source frame. It is committed into
/// the request but grants no capture, scoring, action, or input authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MtgoDuelPerceptionFrameIdentityV1 {
    pub frame_id: u64,
    pub frame_sequence: u64,
}

/// Strict response schema implemented by the exact reviewed classifier
/// executable. This is untrusted process output until the opaque runtime
/// validates its request binding, decision, evidence pixels, and controls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDuelPerceptionProcessResponseV1 {
    pub schema_version: u32,
    pub request_commitment_sha256: String,
    pub reconstruction_audit: MtgoObservationReconstructionAuditV1,
    pub decision: MtgoObservedDecisionV1,
    pub visible_controls: MtgoVisibleActionControlSetV1,
    /// Present for a League or Challenge duel decision and null outside that
    /// scope. When present, the runtime binds and rehashes it against the same
    /// retained frame as the decision and controls.
    pub competitive_lifecycle: Option<MtgoVisibleCompetitiveLifecycleSnapshotV1>,
}

/// Canonical JSON header followed by tightly packed BGRA8 bytes in the private
/// classifier stdin protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDuelPerceptionRequestHeaderV1 {
    pub schema_version: u32,
    pub protocol: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub canonical_width: u32,
    pub canonical_height: u32,
    pub canonical_stride: u32,
    pub canonical_byte_length: usize,
    pub canonical_bgra8_sha256: String,
    pub source_manifest_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub source_frame_profile_binding_sha256: String,
    pub perception_profile_commitment_sha256: String,
    pub perception_profile_admission_commitment_sha256: String,
    pub runtime_identity_commitment_sha256: String,
    pub perception_pipeline_binary_sha256: String,
    pub classifier_assets_manifest_sha256: String,
    pub card_database_profile_sha256: String,
}

/// Canonical header for one pinned gesture-target runtime request. The exact
/// BGRA8 frame follows this JSON header on the private stdin protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDuelGestureTargetRequestHeaderV1 {
    pub schema_version: u32,
    pub protocol: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub canonical_width: u32,
    pub canonical_height: u32,
    pub canonical_stride: u32,
    pub canonical_byte_length: usize,
    pub canonical_bgra8_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub perception_result_commitment_sha256: String,
    pub decision_commitment_sha256: String,
    pub gesture_plan_commitment_sha256: String,
    pub selected_action_family: MtgoDuelActionFamilyV1,
    pub stage_index: u16,
    pub primitive: MtgoDuelGesturePrimitiveV1,
    pub gesture_evaluation_commitment_sha256: String,
    pub gesture_profile_admission_commitment_sha256: String,
    pub runtime_identity_commitment_sha256: String,
    pub gesture_target_runtime_binary_sha256: String,
    pub gesture_target_assets_manifest_sha256: String,
}

/// Strict output schema for the pinned gesture-target runtime. The target set
/// remains untrusted until the opaque Windows invocation validates the exact
/// request binding and rehashes every region from retained pixels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDuelGestureTargetProcessResponseV1 {
    pub schema_version: u32,
    pub request_commitment_sha256: String,
    pub target_set: MtgoVisibleDuelGestureTargetSetV1,
}

/// Structurally checked protocol request metadata. This does not attest that a
/// caller owns a DXGI frame and it retains no pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedUntrustedMtgoDuelPerceptionRequestV1 {
    header: MtgoDuelPerceptionRequestHeaderV1,
    request_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedUntrustedMtgoDuelGestureTargetRequestV1 {
    header: MtgoDuelGestureTargetRequestHeaderV1,
    request_commitment_sha256: String,
}

impl CheckedUntrustedMtgoDuelGestureTargetRequestV1 {
    pub fn header_v1(&self) -> &MtgoDuelGestureTargetRequestHeaderV1 {
        &self.header
    }

    pub fn request_commitment_sha256_v1(&self) -> &str {
        &self.request_commitment_sha256
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

impl CheckedUntrustedMtgoDuelPerceptionRequestV1 {
    pub fn header_v1(&self) -> &MtgoDuelPerceptionRequestHeaderV1 {
        &self.header
    }

    pub fn request_commitment_sha256_v1(&self) -> &str {
        &self.request_commitment_sha256
    }

    pub fn grants_capture_authority_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

/// Copyable commitments for the exact reviewed runtime artifacts. This value
/// contains no executable path, pixels, observation, action, or input method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoVerifiedDuelPerceptionRuntimeCommitmentsV1 {
    pub perception_profile_commitment_sha256: String,
    pub perception_profile_admission_commitment_sha256: String,
    pub perception_pipeline_binary_sha256: String,
    pub classifier_assets_manifest_sha256: String,
    pub card_database_profile_sha256: String,
    pub runtime_identity_commitment_sha256: String,
}

/// Exact on-disk classifier runtime matched to an admitted measured profile.
///
/// The paths and runtime handle remain private. The constructor hashes all
/// three artifacts, and each invocation rechecks them before and after the
/// child process. Production cannot construct the required admitted profile
/// while its separate perception-evaluation ratification root is empty.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoVerifiedDuelPerceptionRuntimeV1;
/// let _forged = OpaqueMtgoVerifiedDuelPerceptionRuntimeV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoVerifiedDuelPerceptionRuntimeV1;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoVerifiedDuelPerceptionRuntimeV1>();
/// ```
pub struct OpaqueMtgoVerifiedDuelPerceptionRuntimeV1 {
    executable_path: PathBuf,
    classifier_assets_manifest_path: PathBuf,
    card_database_profile_path: PathBuf,
    commitments: MtgoVerifiedDuelPerceptionRuntimeCommitmentsV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoVerifiedDuelGestureTargetRuntimeCommitmentsV1 {
    pub gesture_evaluation_commitment_sha256: String,
    pub gesture_profile_admission_commitment_sha256: String,
    pub perception_profile_admission_commitment_sha256: String,
    pub gesture_target_runtime_binary_sha256: String,
    pub gesture_target_assets_manifest_sha256: String,
    pub runtime_identity_commitment_sha256: String,
}

/// Exact on-disk gesture-target runtime matched to the admitted reviewed
/// all-family profile. This handle is move-only and exposes no path or process
/// control. Verification alone does not classify a frame or authorize input.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1>();
/// ```
pub struct OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1 {
    executable_path: PathBuf,
    assets_manifest_path: PathBuf,
    commitments: MtgoVerifiedDuelGestureTargetRuntimeCommitmentsV1,
}

impl OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1 {
    pub fn commitments_v1(&self) -> MtgoVerifiedDuelGestureTargetRuntimeCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

impl OpaqueMtgoVerifiedDuelPerceptionRuntimeV1 {
    pub fn commitments_v1(&self) -> MtgoVerifiedDuelPerceptionRuntimeCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

/// Copyable commitments for one exact opaque source, runtime invocation, and
/// structurally validated visible duel decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoAdmittedDuelPerceptionCommitmentsV1 {
    pub source_frame: MtgoAdmittedDuelVisibleFrameCommitmentsV1,
    pub runtime_identity_commitment_sha256: String,
    pub request_commitment_sha256: String,
    pub decision_commitment_sha256: String,
    pub competitive_lifecycle_snapshot_commitment_sha256: Option<String>,
    pub perception_result_commitment_sha256: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
}

/// One admitted visible frame retained through the exact measured classifier
/// and structural decision validator.
///
/// The type is move-only and exposes commitments only. The source pixels and
/// validated observation remain private for the subsequent scorer boundary.
/// It has no coordinates, control resolution, input, or event-entry method.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoAdmittedDuelPerceptionV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoAdmittedDuelPerceptionV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoAdmittedDuelPerceptionV1;
/// fn cannot_extract(value: &OpaqueMtgoAdmittedDuelPerceptionV1) {
///     let _ = value.canonical_bgra8();
///     let _ = value.observation();
/// }
/// ```
pub struct OpaqueMtgoAdmittedDuelPerceptionV1 {
    pub(super) source_frame: OpaqueMtgoAdmittedDuelVisibleFrameV1,
    pub(super) validated_decision: ValidatedMtgoObservedDecisionV1,
    #[allow(dead_code)]
    source_candidate: Option<CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1>,
    pub(super) decision_record: MtgoObservedDecisionV1,
    pub(super) visible_controls: MtgoVisibleActionControlSetV1,
    competitive_lifecycle: Option<CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1>,
    runtime_identity_commitment_sha256: String,
    request_commitment_sha256: String,
    perception_result_commitment_sha256: String,
}

impl OpaqueMtgoAdmittedDuelPerceptionV1 {
    pub fn commitments_v1(&self) -> MtgoAdmittedDuelPerceptionCommitmentsV1 {
        MtgoAdmittedDuelPerceptionCommitmentsV1 {
            source_frame: self.source_frame.commitments_v1(),
            runtime_identity_commitment_sha256: self.runtime_identity_commitment_sha256.clone(),
            request_commitment_sha256: self.request_commitment_sha256.clone(),
            decision_commitment_sha256: self
                .validated_decision
                .decision_commitment_sha256()
                .to_owned(),
            competitive_lifecycle_snapshot_commitment_sha256: self
                .competitive_lifecycle
                .as_ref()
                .map(|lifecycle| lifecycle.snapshot_commitment_sha256().to_owned()),
            perception_result_commitment_sha256: self.perception_result_commitment_sha256.clone(),
            frame_id: self.validated_decision.frame_id(),
            frame_sequence: self.validated_decision.frame_sequence(),
        }
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    /// Produces the owned player-visible current-state and ordered-action
    /// contract intended for the future kernel scorer. Capture provenance and
    /// the complete kernel observation remain sealed in this opaque value.
    pub fn player_visible_duel_decision_input_v1(
        &self,
    ) -> Result<
        mtgo_blackbox_v1::MtgoPlayerVisibleDuelDecisionInputV1,
        mtgo_blackbox_v1::MtgoContractErrorV1,
    > {
        mtgo_blackbox_v1::build_player_visible_duel_decision_input_v1(&self.validated_decision)
    }

    pub(crate) fn competitive_lifecycle_v1(
        &self,
    ) -> Option<&CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1> {
        self.competitive_lifecycle.as_ref()
    }
}

/// Public result from one exact player-visible-only gameplay selection.
/// Capture, deployment, frame, and adapter-lineage commitments remain private
/// inside the move-only selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPlayerVisibleDuelModelSelectionResultV1 {
    pub selected_index: usize,
    pub selected_logit_f32_bits: u32,
    pub value_f32_bits: u32,
}

/// Move-only result of running the player-visible-only scorer over one opaque
/// admitted MTGO perception. It exposes the selected visible action and finite
/// model outputs, but no source observation, integrity commitments, pixels,
/// coordinates, control resolver, event session, or input path.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPlayerVisibleDuelModelSelectionV1;
/// fn cannot_act(value: OpaqueMtgoPlayerVisibleDuelModelSelectionV1) {
///     let _ = value.observation();
///     let _ = value.selected_semantic();
///     let _ = value.input_command();
///     let _ = value.event_session();
/// }
/// ```
pub struct OpaqueMtgoPlayerVisibleDuelModelSelectionV1 {
    #[allow(dead_code)]
    perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    #[allow(dead_code)]
    selection: Option<CheckedUntrustedMtgoPlayerVisibleProfileBoundDuelModelSelectionV1>,
    selected_action: MtgoPlayerVisibleDuelActionV1,
    result: MtgoPlayerVisibleDuelModelSelectionResultV1,
}

impl OpaqueMtgoPlayerVisibleDuelModelSelectionV1 {
    pub fn selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.selected_action
    }

    pub fn result_v1(&self) -> MtgoPlayerVisibleDuelModelSelectionResultV1 {
        self.result.clone()
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_session_recovery_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Move-only result of scoring one opaque perception and resolving the chosen
/// visible action to exactly one enabled control on that same frame. The
/// control identity, coordinates, frame facts, and all integrity commitments
/// remain private. No input or event authority is exposed.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPlayerVisibleDuelResolvedControlV1;
/// fn cannot_read_control(value: OpaqueMtgoPlayerVisibleDuelResolvedControlV1) {
///     let _ = value.control_id();
///     let _ = value.frame_sequence();
///     let _ = value.rect_client_px();
///     let _ = value.input_command();
/// }
/// ```
pub struct OpaqueMtgoPlayerVisibleDuelResolvedControlV1 {
    #[allow(dead_code)]
    selection: OpaqueMtgoPlayerVisibleDuelModelSelectionV1,
    #[allow(dead_code)]
    resolved: CheckedUntrustedMtgoPlayerVisibleResolvedActionControlV1,
    selected_action: MtgoPlayerVisibleDuelActionV1,
    result: MtgoPlayerVisibleDuelModelSelectionResultV1,
}

impl OpaqueMtgoPlayerVisibleDuelResolvedControlV1 {
    pub fn selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.selected_action
    }

    pub fn result_v1(&self) -> MtgoPlayerVisibleDuelModelSelectionResultV1 {
        self.result.clone()
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_session_recovery_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Copyable commitments for one owner-readable League or Challenge launch
/// identity derived from the exact opaque perception frame. This telemetry is
/// not itself an authorization or input capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoOpaqueCompetitiveLaunchIdentityCommitmentsV1 {
    pub source_capture_commitment_sha256: String,
    pub perception_result_commitment_sha256: String,
    pub lifecycle_snapshot_commitment_sha256: String,
    pub lifecycle_evaluation_commitment_sha256: String,
    pub lifecycle_profile_admission_commitment_sha256: String,
    pub process_continuity_commitment_sha256: String,
    pub window_continuity_commitment_sha256: String,
    pub window_title_sha256: String,
    pub event_label_region_sha256: String,
    pub launch_identity_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub captured_at_unix_millis: u128,
}

/// Exact visible match identity for an attended competitive launch. Only the
/// opaque frame-and-lifecycle binder can construct this move-only value. The
/// event label remains a human-reviewed interpretation of the committed pixel
/// region, while the opponent and visible match IDs are parsed from the exact
/// stable MTGO window title.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveLaunchIdentityV1;
/// let _forged = OpaqueMtgoCompetitiveLaunchIdentityV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveLaunchIdentityV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveLaunchIdentityV1>();
/// ```
pub struct OpaqueMtgoCompetitiveLaunchIdentityV1 {
    commitments: MtgoOpaqueCompetitiveLaunchIdentityCommitmentsV1,
    event_display_label: String,
    opponent_display_name: String,
    visible_match_id: String,
    visible_game_id: String,
    event_identity_sha256: String,
    match_identity_sha256: String,
    entry_authorization_sha256: String,
}

impl OpaqueMtgoCompetitiveLaunchIdentityV1 {
    pub fn commitments_v1(&self) -> MtgoOpaqueCompetitiveLaunchIdentityCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub(crate) fn event_display_label_v1(&self) -> &str {
        &self.event_display_label
    }

    pub(crate) fn opponent_display_name_v1(&self) -> &str {
        &self.opponent_display_name
    }

    pub(crate) fn visible_match_id_v1(&self) -> &str {
        &self.visible_match_id
    }

    pub(crate) fn visible_game_id_v1(&self) -> &str {
        &self.visible_game_id
    }

    pub(crate) fn event_identity_sha256_v1(&self) -> &str {
        &self.event_identity_sha256
    }

    pub(crate) fn match_identity_sha256_v1(&self) -> &str {
        &self.match_identity_sha256
    }

    pub(crate) fn entry_authorization_sha256_v1(&self) -> &str {
        &self.entry_authorization_sha256
    }
}

/// Binds the owner-readable launch identity to the exact retained DXGI pixels,
/// exact classifier result, and same-frame competitive lifecycle snapshot.
/// Every lifecycle fact region is rehashed against the retained pixels. This
/// performs no OCR and grants no input or event-entry authority.
pub(crate) fn bind_opaque_duel_perception_to_competitive_launch_identity_v1(
    perception: &OpaqueMtgoAdmittedDuelPerceptionV1,
    lifecycle_profile: &AdmittedMtgoCompetitiveDuelLifecycleProfileV1,
    event_display_label: String,
    event_label_rect_client_px: MtgoRectPxV1,
    entry_authorization_sha256: String,
) -> Result<OpaqueMtgoCompetitiveLaunchIdentityV1, String> {
    let lifecycle = perception
        .competitive_lifecycle_v1()
        .ok_or("competitive launch identity requires classifier-bound lifecycle pixels")?;
    let perception_commitments = perception.commitments_v1();
    if lifecycle_profile.duel_perception_profile_commitment_sha256()
        != perception_commitments
            .source_frame
            .perception_profile_commitment_sha256
        || lifecycle_profile.duel_perception_profile_admission_commitment_sha256()
            != perception_commitments
                .source_frame
                .perception_profile_admission_commitment_sha256
    {
        return Err(
            "competitive lifecycle evaluation does not bind the exact duel perception profile"
                .to_owned(),
        );
    }
    let source = &perception.source_frame.source_frame;
    let source_capture = &perception_commitments.source_frame.source_capture;
    let source_size = MtgoSizePxV1 {
        width: source_capture.canonical_width,
        height: source_capture.canonical_height,
    };
    validate_competitive_lifecycle_against_duel_pixels_v1(
        lifecycle,
        MtgoDuelPerceptionFrameIdentityV1 {
            frame_id: perception_commitments.frame_id,
            frame_sequence: perception_commitments.frame_sequence,
        },
        &source_capture.canonical_bgra8_sha256,
        &source_size,
        &source.canonical_bgra8,
    )?;

    validate_competitive_launch_display_label_v1(&event_display_label, 160, "event display label")?;
    let required_mode_word = match lifecycle.event_kind() {
        MtgoCompetitiveEventKindV1::League => "league",
        MtgoCompetitiveEventKindV1::Challenge => "challenge",
    };
    if !event_display_label
        .to_ascii_lowercase()
        .contains(required_mode_word)
    {
        return Err(
            "event display label does not identify the selected competitive mode".to_owned(),
        );
    }
    if event_label_rect_client_px.width < 8 || event_label_rect_client_px.height < 8 {
        return Err("event display label region is too small for human review".to_owned());
    }
    let match_surface_rect = lifecycle
        .visible_facts_v1()
        .iter()
        .find(|fact| fact.kind == MtgoLifecycleVisibleFactKindV1::MatchSurfaceVisible)
        .map(|fact| &fact.rect_client_px)
        .ok_or("match lifecycle is missing its visible match surface")?;
    if !rect_contains_rect_v1(match_surface_rect, &event_label_rect_client_px)? {
        return Err("event display label region is outside the visible match surface".to_owned());
    }
    let event_label_region_sha256 = visible_frame_region_content_sha256_v1(
        &source.canonical_bgra8,
        &source_size,
        &event_label_rect_client_px,
    )
    .map_err(|error| format!("hash competitive event label pixels: {error}"))?;

    let manifest = &source.manifest;
    if manifest.pre.title != manifest.post.title {
        return Err("competitive launch window title changed during capture".to_owned());
    }
    let (opponent_display_name, visible_match_id, visible_game_id) =
        parse_competitive_duel_window_title_v1(
            &manifest.pre.title,
            &manifest.expected_game_format,
        )?;
    validate_competitive_launch_display_label_v1(
        &opponent_display_name,
        64,
        "opponent display name",
    )?;
    if !looks_like_lower_sha256_v1(&entry_authorization_sha256) {
        return Err("competitive entry authorization must be lowercase SHA-256".to_owned());
    }
    let event_identity_sha256 = lifecycle
        .event_identity_sha256_v1()
        .ok_or("match lifecycle is missing the event identity")?
        .to_owned();
    let match_identity_sha256 = lifecycle
        .match_identity_sha256_v1()
        .ok_or("match lifecycle is missing the match identity")?
        .to_owned();
    if entry_authorization_sha256 == event_identity_sha256
        || entry_authorization_sha256 == match_identity_sha256
    {
        return Err("entry, event, and match identities must be distinct".to_owned());
    }
    let game_number = lifecycle
        .game_number_v1()
        .ok_or("match lifecycle is missing the game number")?;
    let rect_json = serde_json::to_vec(&event_label_rect_client_px)
        .map_err(|error| format!("serialize competitive event label region: {error}"))?;
    let event_kind_json = serde_json::to_vec(&lifecycle.event_kind())
        .map_err(|error| format!("serialize competitive launch mode: {error}"))?;
    let window_title_sha256 = sha256_hex_v1(manifest.pre.title.as_bytes());
    let process_continuity_commitment_sha256 =
        mtgo_process_continuity_commitment_for_frame_v1(source);
    let window_continuity_commitment_sha256 =
        competitive_entry_window_continuity_commitment_for_frame_v1(source)?;
    let launch_identity_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_COMPETITIVE_LAUNCH_IDENTITY_DOMAIN_V1,
        &[
            source_capture.capture_commitment_sha256.as_bytes(),
            perception_commitments
                .perception_result_commitment_sha256
                .as_bytes(),
            lifecycle.snapshot_commitment_sha256().as_bytes(),
            lifecycle_profile.evaluation_commitment_sha256().as_bytes(),
            lifecycle_profile.admission_commitment_sha256().as_bytes(),
            process_continuity_commitment_sha256.as_bytes(),
            window_continuity_commitment_sha256.as_bytes(),
            window_title_sha256.as_bytes(),
            event_display_label.as_bytes(),
            &rect_json,
            event_label_region_sha256.as_bytes(),
            opponent_display_name.as_bytes(),
            visible_match_id.as_bytes(),
            visible_game_id.as_bytes(),
            &event_kind_json,
            event_identity_sha256.as_bytes(),
            match_identity_sha256.as_bytes(),
            &[game_number],
            source_capture
                .captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            entry_authorization_sha256.as_bytes(),
            b"source_bound_owner_review_only_no_input_or_event_entry_authority",
        ],
    );
    Ok(OpaqueMtgoCompetitiveLaunchIdentityV1 {
        commitments: MtgoOpaqueCompetitiveLaunchIdentityCommitmentsV1 {
            source_capture_commitment_sha256: source_capture.capture_commitment_sha256.clone(),
            perception_result_commitment_sha256: perception_commitments
                .perception_result_commitment_sha256,
            lifecycle_snapshot_commitment_sha256: lifecycle.snapshot_commitment_sha256().to_owned(),
            lifecycle_evaluation_commitment_sha256: lifecycle_profile
                .evaluation_commitment_sha256()
                .to_owned(),
            lifecycle_profile_admission_commitment_sha256: lifecycle_profile
                .admission_commitment_sha256()
                .to_owned(),
            process_continuity_commitment_sha256,
            window_continuity_commitment_sha256,
            window_title_sha256,
            event_label_region_sha256,
            launch_identity_commitment_sha256,
            event_kind: lifecycle.event_kind(),
            game_number,
            frame_id: perception_commitments.frame_id,
            frame_sequence: perception_commitments.frame_sequence,
            captured_at_unix_millis: source_capture.captured_at_unix_millis,
        },
        event_display_label,
        opponent_display_name,
        visible_match_id,
        visible_game_id,
        event_identity_sha256,
        match_identity_sha256,
        entry_authorization_sha256,
    })
}

fn validate_competitive_lifecycle_against_duel_pixels_v1(
    lifecycle: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    identity: MtgoDuelPerceptionFrameIdentityV1,
    frame_sha256: &str,
    size: &MtgoSizePxV1,
    canonical_bgra8: &[u8],
) -> Result<(), String> {
    if lifecycle.phase() != MtgoCompetitiveLifecyclePhaseV1::MatchInProgress {
        return Err("competitive duel lifecycle requires match-in-progress pixels".to_owned());
    }
    let client_bounds = lifecycle.client_bounds_v1();
    if lifecycle.frame_id_v1() != identity.frame_id
        || lifecycle.frame_sequence() != identity.frame_sequence
        || lifecycle.frame_sha256_v1() != frame_sha256
        || client_bounds.x != 0
        || client_bounds.y != 0
        || client_bounds.width != size.width
        || client_bounds.height != size.height
    {
        return Err(
            "competitive lifecycle does not describe the exact opaque duel frame".to_owned(),
        );
    }
    for fact in lifecycle.visible_facts_v1() {
        let actual =
            visible_frame_region_content_sha256_v1(canonical_bgra8, size, &fact.rect_client_px)
                .map_err(|error| format!("rehash competitive lifecycle fact pixels: {error}"))?;
        if actual != fact.content_sha256 {
            return Err(
                "competitive lifecycle fact does not match the retained duel pixels".to_owned(),
            );
        }
    }
    Ok(())
}

pub(super) fn parse_competitive_duel_window_title_v1(
    title: &str,
    expected_game_format: &str,
) -> Result<(String, String, String), String> {
    let prefix = format!("(1-on-1): {expected_game_format}: Vs. ");
    let remainder = title
        .strip_prefix(&prefix)
        .ok_or("competitive duel title does not match the admitted game format")?;
    let (opponent, identity) = remainder
        .split_once(" Match #")
        .ok_or("competitive duel title is missing visible match and game IDs")?;
    let (visible_match_id, visible_game_id) = identity
        .split_once(" - Game #")
        .ok_or("competitive duel title has malformed visible match and game IDs")?;
    if opponent.is_empty()
        || opponent.trim() != opponent
        || opponent.contains(',')
        || visible_match_id.is_empty()
        || visible_match_id.len() > 32
        || visible_game_id.is_empty()
        || visible_game_id.len() > 32
        || !visible_match_id.bytes().all(|byte| byte.is_ascii_digit())
        || !visible_game_id.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("competitive duel title identity is invalid".to_owned());
    }
    Ok((
        opponent.to_owned(),
        visible_match_id.to_owned(),
        visible_game_id.to_owned(),
    ))
}

fn validate_competitive_launch_display_label_v1(
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
            "competitive {field} must be nonempty, trimmed, bounded ASCII display text"
        ));
    }
    Ok(())
}

fn rect_contains_rect_v1(outer: &MtgoRectPxV1, inner: &MtgoRectPxV1) -> Result<bool, String> {
    let outer_right = outer
        .x
        .checked_add(outer.width)
        .ok_or("outer rectangle x overflow")?;
    let outer_bottom = outer
        .y
        .checked_add(outer.height)
        .ok_or("outer rectangle y overflow")?;
    let inner_right = inner
        .x
        .checked_add(inner.width)
        .ok_or("inner rectangle x overflow")?;
    let inner_bottom = inner
        .y
        .checked_add(inner.height)
        .ok_or("inner rectangle y overflow")?;
    Ok(inner.width > 0
        && inner.height > 0
        && inner.x >= outer.x
        && inner.y >= outer.y
        && inner_right <= outer_right
        && inner_bottom <= outer_bottom)
}

/// Copyable scoring telemetry without an observation, semantic, coordinate, or
/// input conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MtgoOpaqueDuelModelSelectionCommitmentsV1 {
    pub perception_result_commitment_sha256: String,
    pub decision_commitment_sha256: String,
    pub deployment_commitment_sha256: String,
    pub selection_commitment_sha256: String,
    pub opaque_selection_commitment_sha256: String,
    pub selected_index: usize,
    pub selected_logit_f32_bits: u32,
    pub value_f32_bits: u32,
}

/// The exact opaque source and validated decision retained through model
/// scoring. It still has no public semantic, coordinate, input, or event-entry
/// conversion.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoProfileBoundDuelModelSelectionV1;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoProfileBoundDuelModelSelectionV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoProfileBoundDuelModelSelectionV1;
/// fn cannot_act(value: &OpaqueMtgoProfileBoundDuelModelSelectionV1) {
///     let _ = value.selected_semantic();
///     let _ = value.target_point_client_px();
/// }
/// ```
#[allow(dead_code)]
pub(crate) struct OpaqueMtgoProfileBoundDuelModelSelectionV1 {
    pub(super) perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    pub(super) selection: Option<CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1>,
    commitments: MtgoOpaqueDuelModelSelectionCommitmentsV1,
    deployment_commitment_sha256: String,
    opaque_selection_commitment_sha256: String,
}

#[allow(dead_code)]
impl OpaqueMtgoProfileBoundDuelModelSelectionV1 {
    pub(crate) fn commitments_v1(&self) -> MtgoOpaqueDuelModelSelectionCommitmentsV1 {
        self.commitments.clone()
    }

    pub(crate) fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub(crate) fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

/// Copyable telemetry for one exact selected semantic resolved to one
/// source-frame pixel region. The region coordinates remain private.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoOpaqueDuelResolvedControlCommitmentsV1 {
    pub opaque_selection_commitment_sha256: String,
    pub decision_commitment_sha256: String,
    pub selection_commitment_sha256: String,
    pub control_resolution_commitment_sha256: String,
    pub profile_bound_resolution_commitment_sha256: String,
    pub opaque_control_resolution_commitment_sha256: String,
    pub control_id: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
}

/// The exact opaque capture, perception, and model selection retained through
/// unique current-frame visible-control resolution. Coordinates remain
/// private for a future authorization-gated actuator in this crate.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoProfileBoundDuelResolvedControlV1;
/// fn cannot_extract(value: &OpaqueMtgoProfileBoundDuelResolvedControlV1) {
///     let _ = value.rect_client_px();
///     let _ = value.input_command();
/// }
/// ```
pub struct OpaqueMtgoProfileBoundDuelResolvedControlV1 {
    pub(super) selection: OpaqueMtgoProfileBoundDuelModelSelectionV1,
    resolved: Option<CheckedUntrustedMtgoProfileBoundResolvedActionControlV1>,
    commitments: MtgoOpaqueDuelResolvedControlCommitmentsV1,
    selected_action_family: MtgoDuelActionFamilyV1,
    selected_semantic_json: Vec<u8>,
    selected_region_content_sha256: String,
    #[allow(dead_code)]
    pub(super) rect_client_px: MtgoRectPxV1,
}

impl OpaqueMtgoProfileBoundDuelResolvedControlV1 {
    pub fn commitments_v1(&self) -> MtgoOpaqueDuelResolvedControlCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    #[allow(dead_code)]
    pub(crate) fn gesture_plan_for_operator_v1(
        &self,
        stages: Vec<MtgoDuelGestureStageV1>,
    ) -> MtgoDuelGesturePlanV1 {
        MtgoDuelGesturePlanV1 {
            schema_version: MTGO_DUEL_GESTURE_PLAN_SCHEMA_V1,
            profile_bound_resolution_commitment_sha256: self
                .commitments
                .profile_bound_resolution_commitment_sha256
                .clone(),
            decision_commitment_sha256: self.commitments.decision_commitment_sha256.clone(),
            selection_commitment_sha256: self.commitments.selection_commitment_sha256.clone(),
            frame_id: self.commitments.frame_id,
            frame_sequence: self.commitments.frame_sequence,
            selected_action_family: self.selected_action_family,
            stage_set_complete: true,
            stages,
        }
    }
}

/// Coordinate-free telemetry for an exact opaque Windows control joined to a
/// separately checked competitive authorization and visible postcondition
/// plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoOpaqueCompetitiveDuelActionPlanCommitmentsV1 {
    pub opaque_control_resolution_commitment_sha256: String,
    pub gesture_plan_commitment_sha256: String,
    pub gesture_stage_count: u16,
    pub postcondition_plan_commitment_sha256: String,
    pub competitive_scope_commitment_sha256: String,
    pub competitive_mode_authorization_commitment_sha256: String,
    pub competitive_authorization_commitment_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub opaque_competitive_action_plan_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub gameplay_authorization_valid_through_frame_sequence: u64,
    pub frame_id: u64,
    pub frame_sequence: u64,
}

/// One exact opaque DXGI frame, model selection, visible control, calibrated
/// postcondition plan, and League or Challenge gameplay authorization.
///
/// Coordinates and postcondition regions remain private in the retained
/// inputs. This type has no input or event-entry conversion. A later actuator
/// must still perform an immediate fresh capture and verify the exact visible
/// action state before one input.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveDuelActionPlanV1;
/// fn cannot_act(value: &OpaqueMtgoCompetitiveDuelActionPlanV1) {
///     let _ = value.rect_client_px();
///     let _ = value.input_command();
///     let _ = value.enter_event();
/// }
/// ```
pub struct OpaqueMtgoCompetitiveDuelActionPlanV1 {
    control: OpaqueMtgoProfileBoundDuelResolvedControlV1,
    gesture: CheckedUntrustedMtgoDuelGesturePlanV1,
    competitive: CheckedUntrustedMtgoCompetitiveGameplayActionPlanV1,
    opaque_competitive_action_plan_commitment_sha256: String,
}

impl OpaqueMtgoCompetitiveDuelActionPlanV1 {
    pub fn commitments_v1(&self) -> MtgoOpaqueCompetitiveDuelActionPlanCommitmentsV1 {
        let control = self.control.commitments_v1();
        let gesture = self.gesture.commitments_v1();
        let postcondition = self.competitive.postcondition_plan_commitments_v1();
        MtgoOpaqueCompetitiveDuelActionPlanCommitmentsV1 {
            opaque_control_resolution_commitment_sha256: control
                .opaque_control_resolution_commitment_sha256,
            gesture_plan_commitment_sha256: gesture.plan_commitment_sha256,
            gesture_stage_count: gesture.stage_count,
            postcondition_plan_commitment_sha256: postcondition.plan_commitment_sha256,
            competitive_scope_commitment_sha256: self
                .competitive
                .competitive_scope_commitment_sha256()
                .to_owned(),
            competitive_mode_authorization_commitment_sha256: self
                .competitive
                .mode_authorization_commitment_sha256()
                .to_owned(),
            competitive_authorization_commitment_sha256: self
                .competitive
                .authorization_commitment_sha256()
                .to_owned(),
            policy_deployment_commitment_sha256: self
                .control
                .selection
                .deployment_commitment_sha256
                .clone(),
            opaque_competitive_action_plan_commitment_sha256: self
                .opaque_competitive_action_plan_commitment_sha256
                .clone(),
            event_kind: self.competitive.event_kind(),
            game_number: self.competitive.game_number(),
            gameplay_authorization_valid_through_frame_sequence: self
                .competitive
                .gameplay_authorization_valid_through_frame_sequence(),
            frame_id: control.frame_id,
            frame_sequence: control.frame_sequence,
        }
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoOpaqueCompetitiveDuelGestureStageCommitmentsV1 {
    pub competitive_action_plan_commitment_sha256: String,
    pub gesture_plan_commitment_sha256: String,
    pub gesture_stage_binding_commitment_sha256: String,
    pub source_perception_result_commitment_sha256: String,
    pub bound_perception_result_commitment_sha256: String,
    pub bound_capture_commitment_sha256: String,
    pub opaque_gesture_stage_commitment_sha256: String,
    pub selected_action_family: MtgoDuelActionFamilyV1,
    pub stage_index: u16,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub target_count: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveDuelGestureVisibleTransitionProbeV1 {
    pub schema_version: u32,
    pub gesture_plan_commitment_sha256: String,
    pub from_stage_binding_commitment_sha256: String,
    pub from_stage_index: u16,
    pub to_stage_index: u16,
    pub before_frame_id: u64,
    pub before_frame_sequence: u64,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub before_frame_region_evidence_id: u64,
    pub after_frame_region_evidence_id: u64,
}

#[derive(Serialize)]
struct PrivateOpaqueGestureTargetRegionV1 {
    role: MtgoDuelGestureTargetRoleV1,
    frame_region_evidence_id: u64,
    rect_client_px: MtgoRectPxV1,
    content_sha256: String,
}

struct ResolvedOpaqueGestureTargetsV1 {
    points_desktop_px: Vec<(i32, i32)>,
    regions: Vec<PrivateOpaqueGestureTargetRegionV1>,
}

/// One checked gesture stage bound to target regions on its required opaque
/// visible frame. The exact click or drag points remain private. This type is
/// a dormant calibration and runtime boundary only, with no input method.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveDuelGestureStageV1;
/// fn cannot_act(value: &OpaqueMtgoCompetitiveDuelGestureStageV1) {
///     let _ = value.target_points_desktop_px();
///     let _ = value.send_input();
/// }
/// ```
pub struct OpaqueMtgoCompetitiveDuelGestureStageV1 {
    _plan: OpaqueMtgoCompetitiveDuelActionPlanV1,
    _continuation_frame: Option<Box<OpaqueMtgoAdmittedDuelPerceptionV1>>,
    _binding: CheckedUntrustedMtgoDuelGestureStageBindingV1,
    commitments: MtgoOpaqueCompetitiveDuelGestureStageCommitmentsV1,
    #[allow(dead_code)]
    target_points_desktop_px: Vec<(i32, i32)>,
    target_regions: Vec<PrivateOpaqueGestureTargetRegionV1>,
}

impl OpaqueMtgoCompetitiveDuelGestureStageV1 {
    pub fn commitments_v1(&self) -> MtgoOpaqueCompetitiveDuelGestureStageCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoOpaqueCompetitiveDuelGestureSequenceCommitmentsV1 {
    pub competitive_action_plan_commitment_sha256: String,
    pub gesture_plan_commitment_sha256: String,
    pub competitive_mode_authorization_commitment_sha256: String,
    pub competitive_match_gameplay_authorization_commitment_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub current_stage_binding_commitment_sha256: String,
    pub current_opaque_stage_commitment_sha256: String,
    pub last_visible_transition_commitment_sha256: Option<String>,
    pub sequence_commitment_sha256: String,
    pub selected_action_family: MtgoDuelActionFamilyV1,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub gameplay_authorization_valid_through_frame_sequence: u64,
    pub current_stage_index: u16,
    pub gesture_stage_count: u16,
    pub observed_stage_count: u16,
    pub current_frame_id: u64,
    pub current_frame_sequence: u64,
}

/// Move-only, coordinate-private chain of gesture targets observed in exact
/// stage order. Advancing the chain requires a strictly newer visible frame
/// and a changed pixel region tied to either the prior or next stage target.
/// It records no input and proves no action causality.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveDuelGestureSequenceV1;
/// fn cannot_act(value: &OpaqueMtgoCompetitiveDuelGestureSequenceV1) {
///     let _ = value.target_points_desktop_px();
///     let _ = value.send_input();
/// }
/// ```
pub struct OpaqueMtgoCompetitiveDuelGestureSequenceV1 {
    current_stage: OpaqueMtgoCompetitiveDuelGestureStageV1,
    commitments: MtgoOpaqueCompetitiveDuelGestureSequenceCommitmentsV1,
}

impl OpaqueMtgoCompetitiveDuelGestureSequenceV1 {
    pub fn commitments_v1(&self) -> MtgoOpaqueCompetitiveDuelGestureSequenceCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn proves_action_causality_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoOpaquePinnedCompetitiveDuelGestureContinuationCommitmentsV1 {
    pub prior_sequence_commitment_sha256: String,
    pub advanced_sequence_commitment_sha256: String,
    pub visible_transition_commitment_sha256: String,
    pub gesture_target_runtime_identity_commitment_sha256: String,
    pub gesture_target_request_commitment_sha256: String,
    pub continuation_commitment_sha256: String,
    pub selected_action_family: MtgoDuelActionFamilyV1,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub stage_index: u16,
    pub gesture_stage_count: u16,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub captured_at_unix_millis: u128,
    pub target_count: u16,
}

/// One strictly newer continuation stage whose target set came from the exact
/// profile-pinned runtime and whose visible transition was rehashed from both
/// retained frames. It grants no input or action-causality authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPinnedCompetitiveDuelGestureContinuationV1;
/// let _forged = OpaqueMtgoPinnedCompetitiveDuelGestureContinuationV1 {};
/// ```
pub struct OpaqueMtgoPinnedCompetitiveDuelGestureContinuationV1 {
    _sequence: OpaqueMtgoCompetitiveDuelGestureSequenceV1,
    commitments: MtgoOpaquePinnedCompetitiveDuelGestureContinuationCommitmentsV1,
}

impl OpaqueMtgoPinnedCompetitiveDuelGestureContinuationV1 {
    pub fn commitments_v1(
        &self,
    ) -> MtgoOpaquePinnedCompetitiveDuelGestureContinuationCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn proves_action_causality_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MtgoOpaqueCompetitiveDuelGestureSourcePreparationCommitmentsV1 {
    pub(crate) source_sequence_commitment_sha256: String,
    pub(crate) prepared_sequence_commitment_sha256: String,
    pub(crate) competitive_action_plan_commitment_sha256: String,
    pub(crate) gesture_plan_commitment_sha256: String,
    pub(crate) competitive_mode_authorization_commitment_sha256: String,
    pub(crate) competitive_match_gameplay_authorization_commitment_sha256: String,
    pub(crate) fresh_stage_binding_commitment_sha256: String,
    pub(crate) fresh_capture_commitment_sha256: String,
    pub(crate) fresh_perception_result_commitment_sha256: String,
    pub(crate) gesture_target_runtime_identity_commitment_sha256: String,
    pub(crate) gesture_target_request_commitment_sha256: String,
    pub(crate) before_input_postcondition_verification_commitment_sha256: String,
    pub(crate) primitive_commitment_sha256: String,
    pub(crate) preparation_commitment_sha256: String,
    pub(crate) selected_action_family: MtgoDuelActionFamilyV1,
    pub(crate) event_kind: MtgoCompetitiveEventKindV1,
    pub(crate) game_number: u8,
    pub(crate) stage_index: u16,
    pub(crate) gesture_stage_count: u16,
    pub(crate) target_count: u16,
    pub(crate) fresh_frame_id: u64,
    pub(crate) fresh_frame_sequence: u64,
    pub(crate) fresh_captured_at_unix_millis: u128,
    pub(crate) gameplay_authorization_valid_through_frame_sequence: u64,
}

/// One source-stage primitive rechecked on a distinct newer opaque perception.
/// Its exact points and pixels remain private, and it has no input conversion.
pub(crate) struct OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1 {
    _sequence: OpaqueMtgoCompetitiveDuelGestureSequenceV1,
    _fresh_perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    _fresh_binding: CheckedUntrustedMtgoDuelGestureStageBindingV1,
    _before_input_postcondition: CheckedUntrustedMtgoCompetitiveGameplayBeforeInputV1,
    pub(crate) commitments: MtgoOpaqueCompetitiveDuelGestureSourcePreparationCommitmentsV1,
    pub(crate) primitive: MtgoDuelGesturePrimitiveV1,
    pub(crate) target_points_desktop_px: Vec<(i32, i32)>,
    #[allow(dead_code)]
    target_regions: Vec<PrivateOpaqueGestureTargetRegionV1>,
    pub(crate) hwnd: u64,
    pub(crate) process_id: u32,
    pub(crate) process_start_filetime_100ns: u64,
    pub(crate) dpi: u32,
    pub(crate) client_rect_desktop_px: SignedRectV1,
    pub(crate) park_x_desktop_px: i32,
    pub(crate) park_y_desktop_px: i32,
}

struct VerifiedDuelGestureTargetRuntimeBindingV1 {
    runtime_identity_commitment_sha256: String,
    request_commitment_sha256: String,
}

/// Coordinate-free telemetry for one immediate fresh-frame preparation of a
/// competitive priority Pass. It proves no input occurred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoOpaqueCompetitiveDuelPassPreparationCommitmentsV1 {
    pub competitive_action_plan_commitment_sha256: String,
    pub competitive_mode_authorization_commitment_sha256: String,
    pub competitive_match_gameplay_authorization_commitment_sha256: String,
    pub before_input_postcondition_verification_commitment_sha256: String,
    pub immediate_capture_commitment_sha256: String,
    pub immediate_perception_result_commitment_sha256: String,
    pub preparation_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub immediate_frame_id: u64,
    pub immediate_frame_sequence: u64,
    pub immediate_captured_at_unix_millis: u128,
}

/// A League or Challenge priority Pass plan rechecked against one immediate
/// fresh opaque capture and classifier result.
///
/// The exact target and cursor park point remain private. This value cannot
/// send input or enter an event. Production duel-profile ratification is empty,
/// so this path is not currently reachable by a production caller.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPreparedCompetitiveDuelPassV1;
/// fn cannot_act(value: &OpaqueMtgoPreparedCompetitiveDuelPassV1) {
///     let _ = value.target_point_client_px();
///     let _ = value.send_input();
///     let _ = value.enter_event();
/// }
/// ```
pub struct OpaqueMtgoPreparedCompetitiveDuelPassV1 {
    _plan: OpaqueMtgoCompetitiveDuelActionPlanV1,
    _current_perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    _before_input_postcondition: CheckedUntrustedMtgoCompetitiveGameplayBeforeInputV1,
    commitments: MtgoOpaqueCompetitiveDuelPassPreparationCommitmentsV1,
    #[allow(dead_code)]
    pub(crate) hwnd: u64,
    #[allow(dead_code)]
    pub(crate) process_id: u32,
    #[allow(dead_code)]
    pub(crate) process_start_filetime_100ns: u64,
    #[allow(dead_code)]
    pub(crate) dpi: u32,
    #[allow(dead_code)]
    pub(crate) client_rect_desktop_px: SignedRectV1,
    #[allow(dead_code)]
    pub(crate) target_x_desktop_px: i32,
    #[allow(dead_code)]
    pub(crate) target_y_desktop_px: i32,
    #[allow(dead_code)]
    pub(crate) park_x_desktop_px: i32,
    #[allow(dead_code)]
    pub(crate) park_y_desktop_px: i32,
}

impl OpaqueMtgoPreparedCompetitiveDuelPassV1 {
    pub fn commitments_v1(&self) -> MtgoOpaqueCompetitiveDuelPassPreparationCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MtgoOpaqueCompetitiveDuelPassConfirmationCommitmentsV1 {
    pub(crate) before_input_verification_commitment_sha256: String,
    pub(crate) after_capture_commitment_sha256: String,
    pub(crate) checked_postcondition_commitment_sha256: String,
    pub(crate) opaque_confirmation_commitment_sha256: String,
    pub(crate) event_kind: MtgoCompetitiveEventKindV1,
    pub(crate) game_number: u8,
    pub(crate) after_frame_id: u64,
    pub(crate) after_frame_sequence: u64,
    pub(crate) postcondition_candidate_count: u32,
}

pub(crate) struct OpaqueMtgoConfirmedCompetitiveDuelPassV1 {
    _before_input_postcondition: CheckedUntrustedMtgoCompetitiveGameplayBeforeInputV1,
    checked_postcondition: CheckedUntrustedMtgoCompetitiveGameplayPostconditionV1,
    _after_frame: OpaqueMtgoAdmittedDuelVisibleFrameV1,
    commitments: MtgoOpaqueCompetitiveDuelPassConfirmationCommitmentsV1,
}

impl OpaqueMtgoConfirmedCompetitiveDuelPassV1 {
    pub(crate) fn commitments_v1(&self) -> MtgoOpaqueCompetitiveDuelPassConfirmationCommitmentsV1 {
        self.commitments.clone()
    }

    pub(crate) fn into_checked_postcondition_v1(
        self,
    ) -> CheckedUntrustedMtgoCompetitiveGameplayPostconditionV1 {
        self.checked_postcondition
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MtgoOpaqueCompetitiveDuelGestureConfirmationCommitmentsV1 {
    pub(crate) before_input_verification_commitment_sha256: String,
    pub(crate) after_capture_commitment_sha256: String,
    pub(crate) checked_postcondition_commitment_sha256: String,
    pub(crate) opaque_confirmation_commitment_sha256: String,
    pub(crate) selected_action_family: MtgoDuelActionFamilyV1,
    pub(crate) event_kind: MtgoCompetitiveEventKindV1,
    pub(crate) game_number: u8,
    pub(crate) after_frame_id: u64,
    pub(crate) after_frame_sequence: u64,
    pub(crate) postcondition_candidate_count: u32,
}

pub(crate) struct OpaqueMtgoConfirmedCompetitiveDuelGestureV1 {
    _before_input_postcondition: CheckedUntrustedMtgoCompetitiveGameplayBeforeInputV1,
    checked_postcondition: CheckedUntrustedMtgoCompetitiveGameplayPostconditionV1,
    _after_frame: OpaqueMtgoAdmittedDuelVisibleFrameV1,
    commitments: MtgoOpaqueCompetitiveDuelGestureConfirmationCommitmentsV1,
}

impl OpaqueMtgoConfirmedCompetitiveDuelGestureV1 {
    pub(crate) fn commitments_v1(
        &self,
    ) -> MtgoOpaqueCompetitiveDuelGestureConfirmationCommitmentsV1 {
        self.commitments.clone()
    }

    pub(crate) fn into_checked_postcondition_v1(
        self,
    ) -> CheckedUntrustedMtgoCompetitiveGameplayPostconditionV1 {
        self.checked_postcondition
    }
}

/// Shared request checker for the capture process and the exact classifier
/// executable. The header must be the canonical serde JSON encoding of the
/// public schema, and the following bytes must be the exact declared BGRA8
/// frame. The result retains commitments only.
pub fn check_untrusted_duel_perception_request_v1(
    canonical_header_json: &[u8],
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoDuelPerceptionRequestV1, String> {
    let header: MtgoDuelPerceptionRequestHeaderV1 =
        serde_json::from_slice(canonical_header_json)
            .map_err(|error| format!("parse duel perception request header: {error}"))?;
    let reencoded = serde_json::to_vec(&header)
        .map_err(|error| format!("serialize duel perception request header: {error}"))?;
    if reencoded != canonical_header_json {
        return Err("duel perception request header is not canonical JSON".to_owned());
    }
    if header.schema_version != 1
        || header.protocol != "mtgo_visible_duel_perception_v1"
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
    {
        return Err("duel perception request identity or geometry is invalid".to_owned());
    }
    let expected_stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("duel perception request stride overflow")?;
    let expected_length = usize::try_from(header.canonical_width)
        .ok()
        .and_then(|width| {
            usize::try_from(header.canonical_height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("duel perception request byte length overflow")?;
    if header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || canonical_bgra8.len() != expected_length
        || header.canonical_bgra8_sha256 != sha256_hex_v1(canonical_bgra8)
    {
        return Err("duel perception request pixels do not match the header".to_owned());
    }
    for value in [
        header.canonical_bgra8_sha256.as_str(),
        header.source_manifest_sha256.as_str(),
        header.source_capture_commitment_sha256.as_str(),
        header.source_frame_profile_binding_sha256.as_str(),
        header.perception_profile_commitment_sha256.as_str(),
        header
            .perception_profile_admission_commitment_sha256
            .as_str(),
        header.runtime_identity_commitment_sha256.as_str(),
        header.perception_pipeline_binary_sha256.as_str(),
        header.classifier_assets_manifest_sha256.as_str(),
        header.card_database_profile_sha256.as_str(),
    ] {
        if !looks_like_lower_sha256_v1(value) {
            return Err("duel perception request contains an invalid commitment".to_owned());
        }
    }
    let request_commitment_sha256 = commitment_v1(
        DUEL_PERCEPTION_REQUEST_DOMAIN_V1,
        &[canonical_header_json, canonical_bgra8],
    );
    Ok(CheckedUntrustedMtgoDuelPerceptionRequestV1 {
        header,
        request_commitment_sha256,
    })
}

/// Checks the canonical gesture-target request envelope and exact following
/// pixels. This is a structural protocol check only and does not attest that a
/// pinned executable produced any later target set.
pub fn check_untrusted_duel_gesture_target_request_v1(
    canonical_header_json: &[u8],
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoDuelGestureTargetRequestV1, String> {
    let header: MtgoDuelGestureTargetRequestHeaderV1 =
        serde_json::from_slice(canonical_header_json)
            .map_err(|error| format!("parse duel gesture-target request header: {error}"))?;
    let reencoded = serde_json::to_vec(&header)
        .map_err(|error| format!("serialize duel gesture-target request header: {error}"))?;
    if reencoded != canonical_header_json {
        return Err("duel gesture-target request header is not canonical JSON".to_owned());
    }
    if header.schema_version != 1
        || header.protocol != "mtgo_visible_duel_gesture_target_v1"
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
        || !canonical_duel_gesture_action_families_v1().contains(&header.selected_action_family)
        || header.stage_index > 31
    {
        return Err("duel gesture-target request identity or geometry is invalid".to_owned());
    }
    let expected_stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("duel gesture-target request stride overflow")?;
    let expected_length = usize::try_from(header.canonical_width)
        .ok()
        .and_then(|width| {
            usize::try_from(header.canonical_height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("duel gesture-target request byte length overflow")?;
    if header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || canonical_bgra8.len() != expected_length
        || header.canonical_bgra8_sha256 != sha256_hex_v1(canonical_bgra8)
    {
        return Err("duel gesture-target request pixels do not match the header".to_owned());
    }
    for value in [
        header.canonical_bgra8_sha256.as_str(),
        header.source_capture_commitment_sha256.as_str(),
        header.perception_result_commitment_sha256.as_str(),
        header.decision_commitment_sha256.as_str(),
        header.gesture_plan_commitment_sha256.as_str(),
        header.gesture_evaluation_commitment_sha256.as_str(),
        header.gesture_profile_admission_commitment_sha256.as_str(),
        header.runtime_identity_commitment_sha256.as_str(),
        header.gesture_target_runtime_binary_sha256.as_str(),
        header.gesture_target_assets_manifest_sha256.as_str(),
    ] {
        if !looks_like_lower_sha256_v1(value) {
            return Err("duel gesture-target request contains an invalid commitment".to_owned());
        }
    }
    let request_commitment_sha256 = commitment_v1(
        DUEL_GESTURE_TARGET_REQUEST_DOMAIN_V1,
        &[canonical_header_json, canonical_bgra8],
    );
    Ok(CheckedUntrustedMtgoDuelGestureTargetRequestV1 {
        header,
        request_commitment_sha256,
    })
}

pub fn verify_duel_perception_runtime_v1(
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    perception_pipeline_binary_path: &Path,
    classifier_assets_manifest_path: &Path,
    card_database_profile_path: &Path,
) -> Result<OpaqueMtgoVerifiedDuelPerceptionRuntimeV1, String> {
    let executable_path = verify_runtime_artifact_v1(
        perception_pipeline_binary_path,
        profile.perception_pipeline_binary_sha256(),
        "perception pipeline binary",
    )?;
    if executable_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("exe"))
        != Some(true)
    {
        return Err("perception pipeline binary must be an exact .exe artifact".to_owned());
    }
    let classifier_assets_manifest_path = verify_runtime_artifact_v1(
        classifier_assets_manifest_path,
        profile.classifier_assets_manifest_sha256(),
        "classifier assets manifest",
    )?;
    let card_database_profile_path = verify_runtime_artifact_v1(
        card_database_profile_path,
        profile.card_database_profile_sha256(),
        "card database profile",
    )?;
    let runtime_identity_commitment_sha256 = commitment_v1(
        DUEL_PERCEPTION_RUNTIME_IDENTITY_DOMAIN_V1,
        &[
            profile.perception_profile_commitment_sha256().as_bytes(),
            profile.admission_commitment_sha256().as_bytes(),
            profile.perception_pipeline_binary_sha256().as_bytes(),
            profile.classifier_assets_manifest_sha256().as_bytes(),
            profile.card_database_profile_sha256().as_bytes(),
            b"verified_artifacts_no_input_or_event_entry_authority",
        ],
    );
    Ok(OpaqueMtgoVerifiedDuelPerceptionRuntimeV1 {
        executable_path,
        classifier_assets_manifest_path,
        card_database_profile_path,
        commitments: MtgoVerifiedDuelPerceptionRuntimeCommitmentsV1 {
            perception_profile_commitment_sha256: profile
                .perception_profile_commitment_sha256()
                .to_owned(),
            perception_profile_admission_commitment_sha256: profile
                .admission_commitment_sha256()
                .to_owned(),
            perception_pipeline_binary_sha256: profile
                .perception_pipeline_binary_sha256()
                .to_owned(),
            classifier_assets_manifest_sha256: profile
                .classifier_assets_manifest_sha256()
                .to_owned(),
            card_database_profile_sha256: profile.card_database_profile_sha256().to_owned(),
            runtime_identity_commitment_sha256,
        },
    })
}

pub fn verify_duel_gesture_target_runtime_v1(
    profile: &AdmittedMtgoDuelGestureProfileV1,
    gesture_target_runtime_binary_path: &Path,
    gesture_target_assets_manifest_path: &Path,
) -> Result<OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1, String> {
    if profile.supported_action_families() != canonical_duel_gesture_action_families_v1() {
        return Err(
            "gesture-target runtime profile does not cover all canonical families".to_owned(),
        );
    }
    let executable_path = verify_runtime_artifact_v1(
        gesture_target_runtime_binary_path,
        profile.gesture_target_runtime_binary_sha256(),
        "gesture-target runtime binary",
    )?;
    if executable_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("exe"))
        != Some(true)
    {
        return Err("gesture-target runtime binary must be an exact .exe artifact".to_owned());
    }
    let assets_manifest_path = verify_runtime_artifact_v1(
        gesture_target_assets_manifest_path,
        profile.gesture_target_assets_manifest_sha256(),
        "gesture-target assets manifest",
    )?;
    let runtime_identity_commitment_sha256 = commitment_v1(
        DUEL_GESTURE_TARGET_RUNTIME_IDENTITY_DOMAIN_V1,
        &[
            profile.evaluation_commitment_sha256().as_bytes(),
            profile.admission_commitment_sha256().as_bytes(),
            profile
                .perception_profile_admission_commitment_sha256()
                .as_bytes(),
            profile.gesture_target_runtime_binary_sha256().as_bytes(),
            profile.gesture_target_assets_manifest_sha256().as_bytes(),
            DUEL_GESTURE_TARGET_PROTOCOL_MAGIC_V1,
            b"verified_target_runtime_identity_no_classification_or_input_authority",
        ],
    );
    Ok(OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1 {
        executable_path,
        assets_manifest_path,
        commitments: MtgoVerifiedDuelGestureTargetRuntimeCommitmentsV1 {
            gesture_evaluation_commitment_sha256: profile.evaluation_commitment_sha256().to_owned(),
            gesture_profile_admission_commitment_sha256: profile
                .admission_commitment_sha256()
                .to_owned(),
            perception_profile_admission_commitment_sha256: profile
                .perception_profile_admission_commitment_sha256()
                .to_owned(),
            gesture_target_runtime_binary_sha256: profile
                .gesture_target_runtime_binary_sha256()
                .to_owned(),
            gesture_target_assets_manifest_sha256: profile
                .gesture_target_assets_manifest_sha256()
                .to_owned(),
            runtime_identity_commitment_sha256,
        },
    })
}

/// Runs only the exact profile-pinned classifier executable. The opaque frame
/// bytes travel over a private child-process pipe and are never returned to the
/// caller. The result must bind the exact frame, recomputed visible-region
/// pixels, supported action families, and existing decision validator.
pub fn perceive_admitted_duel_frame_v1(
    source_frame: OpaqueMtgoAdmittedDuelVisibleFrameV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    runtime: &OpaqueMtgoVerifiedDuelPerceptionRuntimeV1,
    identity: MtgoDuelPerceptionFrameIdentityV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoAdmittedDuelPerceptionV1, String> {
    if identity.frame_id == 0 || identity.frame_sequence == 0 {
        return Err("duel perception frame identity must be nonzero".to_owned());
    }
    if !(100..=60_000).contains(&timeout_ms) {
        return Err("duel perception timeout must be between 100 and 60000 ms".to_owned());
    }
    let frame_commitments = source_frame.commitments_v1();
    if frame_commitments.perception_profile_commitment_sha256
        != profile.perception_profile_commitment_sha256()
        || frame_commitments.perception_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
        || runtime.commitments.perception_profile_commitment_sha256
            != profile.perception_profile_commitment_sha256()
        || runtime
            .commitments
            .perception_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
    {
        return Err("source frame, runtime, and admitted perception profile differ".to_owned());
    }
    verify_duel_perception_runtime_identity_now_v1(runtime)?;

    let source = &source_frame.source_frame;
    let width = source.manifest.frame.canonical_width;
    let height = source.manifest.frame.canonical_height;
    let stride = width
        .checked_mul(4)
        .ok_or("duel perception canonical stride overflow")?;
    if source.manifest.frame.canonical_stride != stride
        || source.manifest.frame.canonical_byte_length != source.canonical_bgra8.len()
        || source.manifest.frame.canonical_bgra8_sha256 != sha256_hex_v1(&source.canonical_bgra8)
    {
        return Err("opaque source canonical pixels no longer match capture metadata".to_owned());
    }
    let header = MtgoDuelPerceptionRequestHeaderV1 {
        schema_version: 1,
        protocol: "mtgo_visible_duel_perception_v1".to_owned(),
        frame_id: identity.frame_id,
        frame_sequence: identity.frame_sequence,
        canonical_width: width,
        canonical_height: height,
        canonical_stride: stride,
        canonical_byte_length: source.canonical_bgra8.len(),
        canonical_bgra8_sha256: source.manifest.frame.canonical_bgra8_sha256.clone(),
        source_manifest_sha256: sha256_hex_v1(
            &serialize_manifest_v2(&source.manifest)
                .map_err(|error| format!("serialize duel source manifest for request: {error}"))?,
        ),
        source_capture_commitment_sha256: source.capture_commitment_sha256.clone(),
        source_frame_profile_binding_sha256: source_frame.frame_profile_binding_sha256.clone(),
        perception_profile_commitment_sha256: profile
            .perception_profile_commitment_sha256()
            .to_owned(),
        perception_profile_admission_commitment_sha256: profile
            .admission_commitment_sha256()
            .to_owned(),
        runtime_identity_commitment_sha256: runtime
            .commitments
            .runtime_identity_commitment_sha256
            .clone(),
        perception_pipeline_binary_sha256: profile.perception_pipeline_binary_sha256().to_owned(),
        classifier_assets_manifest_sha256: profile.classifier_assets_manifest_sha256().to_owned(),
        card_database_profile_sha256: profile.card_database_profile_sha256().to_owned(),
    };
    let header_json = serde_json::to_vec(&header)
        .map_err(|error| format!("serialize duel perception request: {error}"))?;
    let checked_request =
        check_untrusted_duel_perception_request_v1(&header_json, &source.canonical_bgra8)?;
    let request_commitment_sha256 = checked_request.request_commitment_sha256_v1().to_owned();
    let response = invoke_verified_perception_process_v1(
        runtime,
        &header_json,
        &source.canonical_bgra8,
        Duration::from_millis(u64::from(timeout_ms)),
    )?;
    verify_duel_perception_runtime_identity_now_v1(runtime)?;
    let response: MtgoDuelPerceptionProcessResponseV1 =
        serde_json::from_slice(&response).map_err(|error| {
            format!("perception response is not one strict protocol JSON value: {error}")
        })?;
    if response.schema_version != 1
        || response.request_commitment_sha256 != request_commitment_sha256
    {
        return Err("perception response does not bind the exact request".to_owned());
    }
    let validated = validate_and_bind_perception_record_v1(
        &response.decision,
        &source.canonical_bgra8,
        &MtgoSizePxV1 { width, height },
        &source.manifest.frame.canonical_bgra8_sha256,
        identity,
        profile,
    )?;
    validate_visible_control_set_candidate_v1(
        &response.decision,
        &validated,
        &response.visible_controls,
    )?;
    validate_reconstruction_audit_pixels_v1(
        &response.reconstruction_audit,
        &source.canonical_bgra8,
        &MtgoSizePxV1 { width, height },
    )?;
    let source_manifest_json = serialize_manifest_v2(&source.manifest)
        .map_err(|error| format!("serialize opaque duel source manifest: {error}"))?;
    let checked_source = check_untrusted_dxgi_capture_artifact_v1(
        &source_manifest_json,
        &source.canonical_bgra8,
        &source.preview_png,
    )
    .map_err(|error| format!("check opaque duel DXGI source: {error}"))?;
    let checked_audit = validate_dxgi_bound_observation_reconstruction_audit_v1(
        &checked_source,
        response.reconstruction_audit.clone(),
    )
    .map_err(|error| format!("check opaque duel reconstruction audit: {error}"))?;
    let competitive_lifecycle = response
        .competitive_lifecycle
        .clone()
        .map(|snapshot| {
            let lifecycle = validate_visible_competitive_lifecycle_snapshot_v1(snapshot)
                .map_err(|error| format!("validate competitive duel lifecycle: {error}"))?;
            validate_competitive_lifecycle_against_duel_pixels_v1(
                &lifecycle,
                identity,
                &source.manifest.frame.canonical_bgra8_sha256,
                &MtgoSizePxV1 { width, height },
                &source.canonical_bgra8,
            )?;
            Ok::<_, String>(lifecycle)
        })
        .transpose()?;
    let source_candidate = check_untrusted_dxgi_observed_decision_candidate_v1(
        &checked_source,
        &checked_audit,
        profile.checked_runtime_profile_v1(),
        response.decision.clone(),
    )
    .map_err(|error| format!("bind opaque duel decision candidate: {error}"))?;
    if source_candidate.base_decision_commitment_sha256() != validated.decision_commitment_sha256()
    {
        return Err("opaque duel source candidate changed the validated decision".to_owned());
    }
    let competitive_lifecycle_binding = competitive_lifecycle
        .as_ref()
        .map(CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1::snapshot_commitment_sha256)
        .unwrap_or("no_competitive_lifecycle");
    let perception_result_commitment_sha256 = commitment_v1(
        DUEL_PERCEPTION_RESULT_DOMAIN_V1,
        &[
            source_frame.frame_profile_binding_sha256.as_bytes(),
            runtime
                .commitments
                .runtime_identity_commitment_sha256
                .as_bytes(),
            request_commitment_sha256.as_bytes(),
            validated.decision_commitment_sha256().as_bytes(),
            checked_audit.audit_commitment_sha256().as_bytes(),
            source_candidate.candidate_commitment_sha256().as_bytes(),
            competitive_lifecycle_binding.as_bytes(),
            b"opaque_source_retained_no_input_or_event_entry_authority",
        ],
    );
    Ok(OpaqueMtgoAdmittedDuelPerceptionV1 {
        source_frame,
        validated_decision: validated,
        source_candidate: Some(source_candidate),
        decision_record: response.decision,
        visible_controls: response.visible_controls,
        competitive_lifecycle,
        runtime_identity_commitment_sha256: runtime
            .commitments
            .runtime_identity_commitment_sha256
            .clone(),
        request_commitment_sha256,
        perception_result_commitment_sha256,
    })
}

#[allow(dead_code)]
pub(crate) fn score_and_select_opaque_admitted_duel_perception_v1(
    mut perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    deployment: &MtgoExpectedModelDeploymentV1,
    scorer: &mut MtgoNativeCheckpointObservationScorerV1<'_>,
) -> Result<OpaqueMtgoProfileBoundDuelModelSelectionV1, String> {
    let source = perception.source_frame.commitments_v1();
    if source.perception_profile_commitment_sha256 != profile.perception_profile_commitment_sha256()
        || source.perception_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
    {
        return Err("opaque perception and admitted profile differ at scoring".to_owned());
    }
    let source_candidate = perception
        .source_candidate
        .take()
        .ok_or("opaque duel source candidate was already consumed")?;
    let selection = score_and_select_profile_bound_duel_candidate_v1(
        source_candidate,
        profile,
        deployment,
        scorer,
    )
    .map_err(|error| format!("profile-bound duel model scoring failed: {error}"))?;
    finish_opaque_duel_model_selection_v1(perception, selection)
}

/// Runs one opaque admitted perception through the transport-independent
/// player-visible-only scorer seam. The scorer sees only the owned visible
/// state and ordered visible actions. The source candidate remains private in
/// the returned move-only value, which grants no event-session or input
/// authority.
pub fn score_and_select_opaque_player_visible_duel_perception_v1<
    S: MtgoPlayerVisibleDuelScorerV1,
>(
    mut perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<OpaqueMtgoPlayerVisibleDuelModelSelectionV1, String> {
    let source = perception.source_frame.commitments_v1();
    if source.perception_profile_commitment_sha256 != profile.perception_profile_commitment_sha256()
        || source.perception_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
    {
        return Err("opaque perception and admitted profile differ at visible scoring".to_owned());
    }
    let source_candidate = perception
        .source_candidate
        .take()
        .ok_or("opaque duel source candidate was already consumed")?;
    let selection =
        mtgo_blackbox_v1::score_and_select_player_visible_profile_bound_duel_candidate_v1(
            source_candidate,
            profile,
            deployment_commitment_sha256,
            scorer,
        )
        .map_err(|error| format!("player-visible duel model scoring failed: {error}"))?;
    let selected_action = selection.selected_action_v1().clone();
    let result = MtgoPlayerVisibleDuelModelSelectionResultV1 {
        selected_index: selection.selected_index_v1(),
        selected_logit_f32_bits: selection.selected_logit_f32_bits_v1(),
        value_f32_bits: selection.value_f32_bits_v1(),
    };
    Ok(OpaqueMtgoPlayerVisibleDuelModelSelectionV1 {
        perception,
        selection: Some(selection),
        selected_action,
        result,
    })
}

/// Scores one opaque player-visible perception and privately resolves the
/// selected index against the exact complete visible-control set emitted by
/// the same classifier invocation.
pub fn score_select_and_resolve_opaque_player_visible_duel_perception_v1<
    S: MtgoPlayerVisibleDuelScorerV1,
>(
    perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<OpaqueMtgoPlayerVisibleDuelResolvedControlV1, String> {
    let mut selection = score_and_select_opaque_player_visible_duel_perception_v1(
        perception,
        profile,
        deployment_commitment_sha256,
        scorer,
    )?;
    let checked_selection = selection
        .selection
        .take()
        .ok_or("opaque player-visible duel selection was already consumed")?;
    let resolved =
        mtgo_blackbox_v1::resolve_player_visible_profile_bound_selected_visible_control_v1(
            checked_selection,
            selection.perception.visible_controls.clone(),
        )
        .map_err(|error| format!("player-visible duel control resolution failed: {error}"))?;
    if resolved.selected_action_v1() != &selection.selected_action {
        return Err("resolved player-visible duel control changed the selected action".to_owned());
    }
    let selected_action = selection.selected_action.clone();
    let result = selection.result.clone();
    Ok(OpaqueMtgoPlayerVisibleDuelResolvedControlV1 {
        selection,
        resolved,
        selected_action,
        result,
    })
}

/// Scores one retained visible duel perception through the exact loaded
/// checkpoint deployment. The deployment record remains private inside the
/// loaded value, preventing a crossed caller-supplied identity at the live
/// composition boundary.
#[allow(dead_code)]
pub(crate) fn score_and_select_opaque_admitted_duel_perception_with_loaded_deployment_v1(
    mut perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    deployment: &LoadedMtgoNativeCheckpointDeploymentV1,
) -> Result<OpaqueMtgoProfileBoundDuelModelSelectionV1, String> {
    let source = perception.source_frame.commitments_v1();
    if source.perception_profile_commitment_sha256 != profile.perception_profile_commitment_sha256()
        || source.perception_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
    {
        return Err("opaque perception and admitted profile differ at scoring".to_owned());
    }
    let source_candidate = perception
        .source_candidate
        .take()
        .ok_or("opaque duel source candidate was already consumed")?;
    let selection = deployment
        .score_profile_bound_duel_candidate_v1(source_candidate, profile)
        .map_err(|error| format!("profile-bound duel model scoring failed: {error}"))?;
    finish_opaque_duel_model_selection_v1(perception, selection)
}

#[allow(dead_code)]
fn finish_opaque_duel_model_selection_v1(
    perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    selection: CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1,
) -> Result<OpaqueMtgoProfileBoundDuelModelSelectionV1, String> {
    let deployment_commitment_sha256 = selection.deployment_commitment_sha256().to_owned();
    if selection.decision_commitment_sha256()
        != perception.validated_decision.decision_commitment_sha256()
        || selection.selected_index() >= perception.validated_decision.legal_actions().len()
    {
        return Err("duel model selection does not bind the retained decision".to_owned());
    }
    let retained_selected_semantic =
        &perception.validated_decision.legal_actions()[selection.selected_index()];
    let selected_semantic = serde_json::to_vec(retained_selected_semantic)
        .map_err(|error| format!("serialize selected duel semantic: {error}"))?;
    let opaque_selection_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_MODEL_SELECTION_DOMAIN_V1,
        &[
            perception.perception_result_commitment_sha256.as_bytes(),
            perception
                .validated_decision
                .decision_commitment_sha256()
                .as_bytes(),
            deployment_commitment_sha256.as_bytes(),
            selection.selection_commitment_sha256().as_bytes(),
            selection
                .profile_bound_selection_commitment_sha256()
                .as_bytes(),
            &selected_semantic,
            b"opaque_source_retained_no_input_or_event_entry_authority",
        ],
    );
    let commitments = MtgoOpaqueDuelModelSelectionCommitmentsV1 {
        perception_result_commitment_sha256: perception.perception_result_commitment_sha256.clone(),
        decision_commitment_sha256: perception
            .validated_decision
            .decision_commitment_sha256()
            .to_owned(),
        deployment_commitment_sha256: deployment_commitment_sha256.clone(),
        selection_commitment_sha256: selection.selection_commitment_sha256().to_owned(),
        opaque_selection_commitment_sha256: opaque_selection_commitment_sha256.clone(),
        selected_index: selection.selected_index(),
        selected_logit_f32_bits: selection.selected_logit_f32_bits(),
        value_f32_bits: selection.value_f32_bits(),
    };
    Ok(OpaqueMtgoProfileBoundDuelModelSelectionV1 {
        perception,
        selection: Some(selection),
        commitments,
        deployment_commitment_sha256,
        opaque_selection_commitment_sha256,
    })
}

/// Resolves the selected semantic against the complete control set emitted by
/// the same exact classifier invocation. The existing black-box resolver
/// rechecks decision, frame, prompt, enabled state, confidence, legality, and
/// unique selected match. This Windows-side wrapper additionally retains the
/// opaque source frame and its private pixel-region coordinates.
#[allow(dead_code)]
pub(crate) fn resolve_opaque_profile_bound_duel_control_v1(
    mut selection: OpaqueMtgoProfileBoundDuelModelSelectionV1,
) -> Result<OpaqueMtgoProfileBoundDuelResolvedControlV1, String> {
    let profile_bound_selection = selection
        .selection
        .take()
        .ok_or("opaque duel model selection was already consumed")?;
    let selected_semantic_value = selection
        .perception
        .validated_decision
        .legal_actions()
        .get(profile_bound_selection.selected_index())
        .cloned()
        .ok_or("selected duel action is outside the retained legal-action vector")?;
    let selected_candidate = selection
        .perception
        .visible_controls
        .controls
        .iter()
        .find(|candidate| candidate.semantic == selected_semantic_value)
        .ok_or("selected duel action has no retained visible control")?;
    let selected_control_id = selected_candidate.control_id.clone();
    let selected_evidence_id = selected_candidate.frame_region_evidence_id;
    let selected_semantic_json = serde_json::to_vec(&selected_semantic_value)
        .map_err(|error| format!("serialize selected duel semantic: {error}"))?;
    let (rect_client_px, region_content_sha256) =
        frame_region_for_evidence_v1(&selection.perception.decision_record, selected_evidence_id)?;
    let rect_client_px = rect_client_px.clone();
    let region_content_sha256 = region_content_sha256.to_owned();
    let resolved = resolve_profile_bound_selected_visible_control_v1(
        profile_bound_selection,
        selection.perception.visible_controls.clone(),
    )
    .map_err(|error| format!("visible duel control resolution failed: {error}"))?;
    if resolved.control_id() != selected_control_id
        || resolved.frame_id() != selection.perception.validated_decision.frame_id()
        || resolved.frame_sequence() != selection.perception.validated_decision.frame_sequence()
        || resolved.selection_commitment_sha256()
            != selection.commitments.selection_commitment_sha256
    {
        return Err("resolved duel control does not retain the exact opaque selection".to_owned());
    }
    let rect_json = serde_json::to_vec(&rect_client_px)
        .map_err(|error| format!("serialize resolved duel control rectangle: {error}"))?;
    let opaque_control_resolution_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_CONTROL_RESOLUTION_DOMAIN_V1,
        &[
            selection.opaque_selection_commitment_sha256.as_bytes(),
            resolved.control_resolution_commitment_sha256().as_bytes(),
            resolved
                .profile_bound_resolution_commitment_sha256()
                .as_bytes(),
            selected_control_id.as_bytes(),
            &selected_semantic_json,
            &rect_json,
            region_content_sha256.as_bytes(),
            b"opaque_source_retained_coordinates_private_no_input_or_event_entry_authority",
        ],
    );
    let commitments = MtgoOpaqueDuelResolvedControlCommitmentsV1 {
        opaque_selection_commitment_sha256: selection.opaque_selection_commitment_sha256.clone(),
        decision_commitment_sha256: resolved.decision_commitment_sha256().to_owned(),
        selection_commitment_sha256: resolved.selection_commitment_sha256().to_owned(),
        control_resolution_commitment_sha256: resolved
            .control_resolution_commitment_sha256()
            .to_owned(),
        profile_bound_resolution_commitment_sha256: resolved
            .profile_bound_resolution_commitment_sha256()
            .to_owned(),
        opaque_control_resolution_commitment_sha256: opaque_control_resolution_commitment_sha256
            .clone(),
        control_id: resolved.control_id().to_owned(),
        frame_id: resolved.frame_id(),
        frame_sequence: resolved.frame_sequence(),
    };
    Ok(OpaqueMtgoProfileBoundDuelResolvedControlV1 {
        selection,
        resolved: Some(resolved),
        commitments,
        selected_action_family: duel_action_family_v1(&selected_semantic_value),
        selected_semantic_json,
        selected_region_content_sha256: region_content_sha256,
        rect_client_px,
    })
}

/// Builds the calibrated visible postcondition and exact League or Challenge
/// match scope from the same move-only profile-bound control resolution that
/// was produced by the opaque Windows capture path.
///
/// This still creates no input command and grants no event-entry authority.
pub fn prepare_opaque_competitive_duel_action_plan_v1(
    mut control: OpaqueMtgoProfileBoundDuelResolvedControlV1,
    gesture_plan: MtgoDuelGesturePlanV1,
    calibration: MtgoProfileBoundPostconditionCalibrationV1,
    region_set: MtgoProfileBoundPostconditionRegionSetV1,
    mode_authorization: &MtgoAuthorizationScopeV1,
    gameplay_authorization: &MtgoCompetitiveMatchGameplayAuthorizationV1,
) -> Result<OpaqueMtgoCompetitiveDuelActionPlanV1, String> {
    let gesture = validate_profile_bound_duel_gesture_plan_v1(
        control
            .resolved
            .as_ref()
            .ok_or("opaque duel control resolution was already consumed")?,
        gesture_plan,
    )
    .map_err(|error| format!("validate opaque duel gesture plan: {error}"))?;
    let resolved = control
        .resolved
        .take()
        .ok_or("opaque duel control resolution was already consumed")?;
    let postcondition =
        prepare_profile_bound_action_postcondition_plan_v1(resolved, calibration, region_set)
            .map_err(|error| format!("prepare opaque duel postcondition plan: {error}"))?;
    let lifecycle = control
        .selection
        .perception
        .competitive_lifecycle
        .take()
        .ok_or("competitive duel action requires classifier-bound lifecycle pixels")?;
    let competitive = bind_profile_bound_action_plan_to_competitive_match_v1(
        postcondition,
        lifecycle,
        mode_authorization,
        gameplay_authorization,
    )
    .map_err(|error| format!("scope opaque duel action to competitive match: {error}"))?;
    bind_opaque_duel_control_to_competitive_action_plan_v1(control, gesture, competitive)
}

/// Resolves stage zero of an opaque competitive gesture against the exact
/// source decision frame. Every target region is rehashed from retained DXGI
/// pixels and converted to private center points. No input is produced.
pub fn bind_opaque_competitive_duel_source_gesture_stage_v1(
    plan: OpaqueMtgoCompetitiveDuelActionPlanV1,
    target_set: MtgoVisibleDuelGestureTargetSetV1,
) -> Result<OpaqueMtgoCompetitiveDuelGestureStageV1, String> {
    if target_set.stage_index != 0 {
        return Err("source gesture binder accepts stage zero only".to_owned());
    }
    bind_opaque_competitive_duel_gesture_stage_inner_v1(plan, None, target_set)
}

/// Resolves one continuation stage against a distinct strictly newer opaque
/// perception of the same duel decision and window incarnation. It does not
/// imply that an earlier stage was executed and therefore remains dormant and
/// non-actionable until a future sequential actuator consumes it.
pub fn bind_opaque_competitive_duel_continuation_gesture_stage_v1(
    plan: OpaqueMtgoCompetitiveDuelActionPlanV1,
    current_perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    target_set: MtgoVisibleDuelGestureTargetSetV1,
) -> Result<OpaqueMtgoCompetitiveDuelGestureStageV1, String> {
    if target_set.stage_index == 0 {
        return Err("continuation gesture binder requires a later stage".to_owned());
    }
    bind_opaque_competitive_duel_gesture_stage_inner_v1(plan, Some(current_perception), target_set)
}

pub fn begin_opaque_competitive_duel_gesture_sequence_v1(
    source_stage: OpaqueMtgoCompetitiveDuelGestureStageV1,
) -> Result<OpaqueMtgoCompetitiveDuelGestureSequenceV1, String> {
    let stage = source_stage.commitments_v1();
    let action_plan = source_stage._plan.commitments_v1();
    if stage.stage_index != 0 {
        return Err("gesture sequence must begin from stage zero".to_owned());
    }
    let gesture_stage_count = u16::try_from(source_stage._plan.gesture.stages_v1().len())
        .map_err(|_| "gesture stage count exceeds u16")?;
    if gesture_stage_count == 0 {
        return Err("gesture sequence cannot begin from an empty plan".to_owned());
    }
    let selected_family_json = serde_json::to_vec(&stage.selected_action_family)
        .map_err(|error| format!("serialize gesture sequence family: {error}"))?;
    let event_kind_json = serde_json::to_vec(&action_plan.event_kind)
        .map_err(|error| format!("serialize gesture sequence mode: {error}"))?;
    let sequence_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_COMPETITIVE_GESTURE_SEQUENCE_DOMAIN_V1,
        &[
            stage.competitive_action_plan_commitment_sha256.as_bytes(),
            stage.gesture_plan_commitment_sha256.as_bytes(),
            stage.gesture_stage_binding_commitment_sha256.as_bytes(),
            stage.opaque_gesture_stage_commitment_sha256.as_bytes(),
            action_plan
                .competitive_mode_authorization_commitment_sha256
                .as_bytes(),
            action_plan
                .competitive_authorization_commitment_sha256
                .as_bytes(),
            action_plan.policy_deployment_commitment_sha256.as_bytes(),
            selected_family_json.as_slice(),
            event_kind_json.as_slice(),
            &[action_plan.game_number],
            action_plan
                .gameplay_authorization_valid_through_frame_sequence
                .to_be_bytes()
                .as_slice(),
            &gesture_stage_count.to_be_bytes(),
            &1_u16.to_be_bytes(),
            b"source_stage_observed_no_input_or_action_causality",
        ],
    );
    let commitments = MtgoOpaqueCompetitiveDuelGestureSequenceCommitmentsV1 {
        competitive_action_plan_commitment_sha256: stage.competitive_action_plan_commitment_sha256,
        gesture_plan_commitment_sha256: stage.gesture_plan_commitment_sha256,
        competitive_mode_authorization_commitment_sha256: action_plan
            .competitive_mode_authorization_commitment_sha256,
        competitive_match_gameplay_authorization_commitment_sha256: action_plan
            .competitive_authorization_commitment_sha256,
        policy_deployment_commitment_sha256: action_plan.policy_deployment_commitment_sha256,
        current_stage_binding_commitment_sha256: stage.gesture_stage_binding_commitment_sha256,
        current_opaque_stage_commitment_sha256: stage.opaque_gesture_stage_commitment_sha256,
        last_visible_transition_commitment_sha256: None,
        sequence_commitment_sha256,
        selected_action_family: stage.selected_action_family,
        event_kind: action_plan.event_kind,
        game_number: action_plan.game_number,
        gameplay_authorization_valid_through_frame_sequence: action_plan
            .gameplay_authorization_valid_through_frame_sequence,
        current_stage_index: 0,
        gesture_stage_count,
        observed_stage_count: 1,
        current_frame_id: stage.frame_id,
        current_frame_sequence: stage.frame_sequence,
    };
    Ok(OpaqueMtgoCompetitiveDuelGestureSequenceV1 {
        current_stage: source_stage,
        commitments,
    })
}

/// Resolves stage zero with the exact profile-pinned visible-target runtime,
/// then begins the ordinary opaque gesture sequence. This is the production
/// composition seam for callers that already own the complete competitive
/// action plan. It performs no input and returns no coordinates.
#[allow(dead_code)]
pub(crate) fn begin_opaque_competitive_duel_gesture_sequence_from_pinned_runtime_v1(
    plan: OpaqueMtgoCompetitiveDuelActionPlanV1,
    profile: &AdmittedMtgoDuelGestureProfileV1,
    runtime: &OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoCompetitiveDuelGestureSequenceV1, String> {
    let (target_set, _runtime_binding) = invoke_pinned_gesture_target_runtime_for_plan_stage_v1(
        &plan,
        &plan.control.selection.perception,
        profile,
        runtime,
        0,
        timeout_ms,
    )?;
    let source_stage = bind_opaque_competitive_duel_source_gesture_stage_v1(plan, target_set)?;
    begin_opaque_competitive_duel_gesture_sequence_v1(source_stage)
}

/// Runs the exact profile-pinned gesture-target runtime over one retained fresh
/// frame, binds its response to the selected action and source primitive, and
/// then performs the normal visible-pixel recheck. No pixels, targets, points,
/// process handle, or input method are returned to the caller.
pub(crate) fn prepare_opaque_competitive_duel_gesture_source_stage_from_pinned_runtime_v1(
    sequence: OpaqueMtgoCompetitiveDuelGestureSequenceV1,
    fresh_perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    profile: &AdmittedMtgoDuelGestureProfileV1,
    runtime: &OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1, String> {
    if !(100..=60_000).contains(&timeout_ms) {
        return Err("duel gesture-target timeout must be between 100 and 60000 ms".to_owned());
    }
    let sequence_commitments = sequence.commitments_v1();
    if sequence_commitments.current_stage_index != 0
        || sequence_commitments.observed_stage_count != 1
        || sequence_commitments
            .last_visible_transition_commitment_sha256
            .is_some()
    {
        return Err("gesture-target runtime requires an unadvanced source sequence".to_owned());
    }
    if profile.supported_action_families() != canonical_duel_gesture_action_families_v1()
        || !profile
            .supported_action_families()
            .contains(&sequence_commitments.selected_action_family)
        || runtime.commitments.gesture_evaluation_commitment_sha256
            != profile.evaluation_commitment_sha256()
        || runtime
            .commitments
            .gesture_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
        || runtime
            .commitments
            .perception_profile_admission_commitment_sha256
            != profile.perception_profile_admission_commitment_sha256()
    {
        return Err("gesture sequence, runtime, and admitted all-family profile differ".to_owned());
    }
    prepare_opaque_competitive_duel_gesture_current_stage_from_pinned_runtime_v1(
        sequence,
        fresh_perception,
        profile,
        runtime,
        timeout_ms,
    )
}

fn prepare_opaque_competitive_duel_gesture_current_stage_from_pinned_runtime_v1(
    sequence: OpaqueMtgoCompetitiveDuelGestureSequenceV1,
    fresh_perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    profile: &AdmittedMtgoDuelGestureProfileV1,
    runtime: &OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1, String> {
    let stage_index = sequence.commitments.current_stage_index;
    let (target_set, target_runtime) = invoke_pinned_gesture_target_runtime_for_stage_v1(
        &sequence,
        &fresh_perception,
        profile,
        runtime,
        stage_index,
        timeout_ms,
    )?;
    prepare_opaque_competitive_duel_gesture_source_stage_from_fresh_frame_v1(
        sequence,
        fresh_perception,
        target_set,
        target_runtime,
    )
}

/// Rechecks the exact current-stage primitive on one distinct newer opaque duel
/// perception after its complete target set was produced by the verified
/// pinned runtime. This consumes the source sequence, retains all target
/// points privately, and performs no input.
fn prepare_opaque_competitive_duel_gesture_source_stage_from_fresh_frame_v1(
    sequence: OpaqueMtgoCompetitiveDuelGestureSequenceV1,
    fresh_perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    target_set: MtgoVisibleDuelGestureTargetSetV1,
    target_runtime: VerifiedDuelGestureTargetRuntimeBindingV1,
) -> Result<OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1, String> {
    let sequence_commitments = sequence.commitments_v1();
    if sequence_commitments.current_stage_index >= sequence_commitments.gesture_stage_count
        || sequence_commitments.observed_stage_count
            != sequence_commitments.current_stage_index.saturating_add(1)
        || (sequence_commitments.current_stage_index == 0)
            == sequence_commitments
                .last_visible_transition_commitment_sha256
                .is_some()
    {
        return Err(
            "gesture preparation requires one internally consistent current stage".to_owned(),
        );
    }
    let source_stage = &sequence.current_stage;
    let plan = &source_stage._plan;
    let source_perception = &plan.control.selection.perception;
    let prior_perception = source_stage
        ._continuation_frame
        .as_deref()
        .unwrap_or(source_perception);
    let prior_manifest = &prior_perception.source_frame.source_frame.manifest;
    let fresh_manifest = &fresh_perception.source_frame.source_frame.manifest;
    validate_same_duel_window_incarnation_v1(prior_manifest, fresh_manifest)?;

    let fresh_commitments = fresh_perception.commitments_v1();
    if fresh_commitments
        .source_frame
        .source_capture
        .captured_at_unix_millis
        <= prior_manifest.captured_at_unix_millis
        || fresh_commitments
            .source_frame
            .source_capture
            .capture_commitment_sha256
            == source_perception
                .source_frame
                .source_frame
                .capture_commitment_sha256
    {
        return Err("gesture source preparation frame is not a distinct newer capture".to_owned());
    }
    let expected_frame_sequence = sequence_commitments
        .current_frame_sequence
        .checked_add(1)
        .ok_or("gesture source preparation frame sequence overflow")?;
    let expected_frame_id = frame_id_from_capture_commitment_v1(
        &fresh_commitments
            .source_frame
            .source_capture
            .capture_commitment_sha256,
        sequence_commitments.current_frame_id,
    )?;
    if fresh_commitments.frame_id != expected_frame_id
        || fresh_commitments.frame_sequence != expected_frame_sequence
        || target_set.frame_id != fresh_commitments.frame_id
        || target_set.frame_sequence != fresh_commitments.frame_sequence
        || target_set.stage_index != sequence_commitments.current_stage_index
        || expected_frame_sequence
            > sequence_commitments.gameplay_authorization_valid_through_frame_sequence
    {
        return Err(
            "gesture source preparation does not bind the exact next authorized frame".to_owned(),
        );
    }
    if source_perception
        .source_frame
        .perception_profile_commitment_sha256
        != fresh_perception
            .source_frame
            .perception_profile_commitment_sha256
        || source_perception
            .source_frame
            .perception_profile_admission_commitment_sha256
            != fresh_perception
                .source_frame
                .perception_profile_admission_commitment_sha256
        || source_perception.runtime_identity_commitment_sha256
            != fresh_perception.runtime_identity_commitment_sha256
    {
        return Err("gesture source preparation changed perception identity".to_owned());
    }
    if fresh_perception.decision_record.payload != source_perception.decision_record.payload {
        return Err("visible duel decision payload changed before gesture preparation".to_owned());
    }

    let fresh_primary = if sequence_commitments.current_stage_index == 0 {
        let mut matching_controls = Vec::new();
        for candidate in &fresh_perception.visible_controls.controls {
            let semantic_json = serde_json::to_vec(&candidate.semantic)
                .map_err(|error| format!("serialize fresh gesture control semantic: {error}"))?;
            if semantic_json == plan.control.selected_semantic_json {
                matching_controls.push(candidate);
            }
        }
        if matching_controls.len() != 1
            || matching_controls[0].control_id != plan.control.commitments.control_id
        {
            return Err(
                "fresh gesture semantic control is ambiguous or changed identity".to_owned(),
            );
        }
        let selected = matching_controls[0];
        let (fresh_primary_rect, fresh_primary_sha256) = frame_region_for_evidence_v1(
            &fresh_perception.decision_record,
            selected.frame_region_evidence_id,
        )?;
        if fresh_primary_rect != &plan.control.rect_client_px
            || fresh_primary_sha256 != plan.control.selected_region_content_sha256
        {
            return Err("fresh gesture semantic control geometry or pixels changed".to_owned());
        }
        Some((fresh_primary_rect, fresh_primary_sha256))
    } else {
        None
    };

    let fresh_binding = if sequence_commitments.current_stage_index == 0 {
        recheck_visible_duel_gesture_source_stage_v1(
            &plan.gesture,
            &fresh_perception.validated_decision,
            target_set.clone(),
        )
        .map_err(|error| format!("recheck fresh gesture source stage: {error}"))?
    } else {
        bind_visible_duel_gesture_stage_v1(
            &plan.gesture,
            &fresh_perception.validated_decision,
            target_set.clone(),
        )
        .map_err(|error| format!("recheck fresh gesture continuation stage: {error}"))?
    };
    let primitive = plan
        .gesture
        .stages_v1()
        .get(usize::from(sequence_commitments.current_stage_index))
        .ok_or("gesture current stage is absent from the exact plan")?
        .primitive
        .clone();
    let resolved_targets = resolve_opaque_gesture_target_points_v1(
        &target_set,
        &primitive,
        &fresh_perception,
        fresh_primary,
    )?;
    let fresh_manifest_json = serialize_manifest_v2(fresh_manifest)
        .map_err(|error| format!("serialize fresh gesture capture manifest: {error}"))?;
    let fresh_output_bounds = MtgoSignedRectDesktopPxV1 {
        left: fresh_manifest.output.bounds_desktop_px.left,
        top: fresh_manifest.output.bounds_desktop_px.top,
        width: fresh_manifest.output.bounds_desktop_px.width()?,
        height: fresh_manifest.output.bounds_desktop_px.height()?,
    };
    let fresh_output_identity_sha256 = preview_output_identity_commitment_v1(
        &fresh_manifest.output.device_name,
        &fresh_output_bounds,
    )
    .map_err(|error| format!("fresh gesture output identity is invalid: {error}"))?;
    let before_input_postcondition = check_untrusted_competitive_gameplay_before_input_pixels_v1(
        &plan.competitive,
        MtgoProfileBoundPostconditionBeforeInputFrameV1 {
            schema_version: MTGO_PROFILE_BOUND_POSTCONDITION_BEFORE_INPUT_FRAME_SCHEMA_V1,
            plan_commitment_sha256: plan
                .competitive
                .postcondition_plan_commitment_sha256()
                .to_owned(),
            frame_id: fresh_commitments.frame_id,
            frame_sequence: fresh_commitments.frame_sequence,
            manifest_sha256: sha256_hex_v1(&fresh_manifest_json),
            output_identity_sha256: fresh_output_identity_sha256,
            perception_profile_admission_commitment_sha256: fresh_perception
                .source_frame
                .perception_profile_admission_commitment_sha256
                .clone(),
            client_size_px: MtgoSizePxV1 {
                width: fresh_manifest.frame.canonical_width,
                height: fresh_manifest.frame.canonical_height,
            },
            capture_role: MtgoDxgiCaptureRoleV2::ActingPlayerDuel,
        },
        &fresh_perception.source_frame.source_frame.canonical_bgra8,
    )
    .map_err(|error| format!("verify fresh gesture postcondition baseline: {error}"))?;
    let binding_commitments = fresh_binding.commitments_v1();
    if binding_commitments.stage_index != sequence_commitments.current_stage_index
        || binding_commitments.frame_id != fresh_commitments.frame_id
        || binding_commitments.frame_sequence != fresh_commitments.frame_sequence
        || binding_commitments.target_count as usize != resolved_targets.points_desktop_px.len()
    {
        return Err("fresh gesture binding changed its exact stage or targets".to_owned());
    }

    let primitive_json = serde_json::to_vec(&primitive)
        .map_err(|error| format!("serialize fresh gesture primitive: {error}"))?;
    let primitive_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_COMPETITIVE_GESTURE_SOURCE_PREPARATION_DOMAIN_V1,
        &[b"primitive", &primitive_json],
    );
    let family_json = serde_json::to_vec(&sequence_commitments.selected_action_family)
        .map_err(|error| format!("serialize fresh gesture family: {error}"))?;
    let event_kind_json = serde_json::to_vec(&sequence_commitments.event_kind)
        .map_err(|error| format!("serialize fresh gesture event kind: {error}"))?;
    let preparation_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_COMPETITIVE_GESTURE_SOURCE_PREPARATION_DOMAIN_V1,
        &[
            sequence_commitments.sequence_commitment_sha256.as_bytes(),
            sequence_commitments
                .competitive_action_plan_commitment_sha256
                .as_bytes(),
            sequence_commitments
                .gesture_plan_commitment_sha256
                .as_bytes(),
            sequence_commitments
                .competitive_mode_authorization_commitment_sha256
                .as_bytes(),
            sequence_commitments
                .competitive_match_gameplay_authorization_commitment_sha256
                .as_bytes(),
            binding_commitments.binding_commitment_sha256.as_bytes(),
            fresh_commitments
                .source_frame
                .source_capture
                .capture_commitment_sha256
                .as_bytes(),
            fresh_commitments
                .perception_result_commitment_sha256
                .as_bytes(),
            target_runtime.runtime_identity_commitment_sha256.as_bytes(),
            target_runtime.request_commitment_sha256.as_bytes(),
            before_input_postcondition
                .verification_commitment_sha256()
                .as_bytes(),
            primitive_commitment_sha256.as_bytes(),
            &family_json,
            &event_kind_json,
            &[sequence_commitments.game_number],
            &binding_commitments.stage_index.to_be_bytes(),
            &sequence_commitments.gesture_stage_count.to_be_bytes(),
            &binding_commitments.target_count.to_be_bytes(),
            &fresh_commitments.frame_id.to_be_bytes(),
            &fresh_commitments.frame_sequence.to_be_bytes(),
            &sequence_commitments
                .gameplay_authorization_valid_through_frame_sequence
                .to_be_bytes(),
            b"fresh_source_stage_rechecked_private_points_no_input_or_action_causality",
        ],
    );
    let prepared_sequence_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_COMPETITIVE_GESTURE_SEQUENCE_DOMAIN_V1,
        &[
            sequence_commitments.sequence_commitment_sha256.as_bytes(),
            binding_commitments.binding_commitment_sha256.as_bytes(),
            fresh_commitments
                .source_frame
                .source_capture
                .capture_commitment_sha256
                .as_bytes(),
            fresh_commitments
                .perception_result_commitment_sha256
                .as_bytes(),
            primitive_commitment_sha256.as_bytes(),
            preparation_commitment_sha256.as_bytes(),
            &fresh_commitments.frame_id.to_be_bytes(),
            &fresh_commitments.frame_sequence.to_be_bytes(),
            b"same_stage_refreshed_immediately_before_input_no_action_causality",
        ],
    );
    let commitments = MtgoOpaqueCompetitiveDuelGestureSourcePreparationCommitmentsV1 {
        source_sequence_commitment_sha256: sequence_commitments.sequence_commitment_sha256,
        prepared_sequence_commitment_sha256,
        competitive_action_plan_commitment_sha256: sequence_commitments
            .competitive_action_plan_commitment_sha256,
        gesture_plan_commitment_sha256: sequence_commitments.gesture_plan_commitment_sha256,
        competitive_mode_authorization_commitment_sha256: sequence_commitments
            .competitive_mode_authorization_commitment_sha256,
        competitive_match_gameplay_authorization_commitment_sha256: sequence_commitments
            .competitive_match_gameplay_authorization_commitment_sha256,
        fresh_stage_binding_commitment_sha256: binding_commitments.binding_commitment_sha256,
        fresh_capture_commitment_sha256: fresh_commitments
            .source_frame
            .source_capture
            .capture_commitment_sha256,
        fresh_perception_result_commitment_sha256: fresh_commitments
            .perception_result_commitment_sha256,
        gesture_target_runtime_identity_commitment_sha256: target_runtime
            .runtime_identity_commitment_sha256,
        gesture_target_request_commitment_sha256: target_runtime.request_commitment_sha256,
        before_input_postcondition_verification_commitment_sha256: before_input_postcondition
            .verification_commitment_sha256()
            .to_owned(),
        primitive_commitment_sha256,
        preparation_commitment_sha256,
        selected_action_family: sequence_commitments.selected_action_family,
        event_kind: sequence_commitments.event_kind,
        game_number: sequence_commitments.game_number,
        stage_index: binding_commitments.stage_index,
        gesture_stage_count: sequence_commitments.gesture_stage_count,
        target_count: binding_commitments.target_count,
        fresh_frame_id: fresh_commitments.frame_id,
        fresh_frame_sequence: fresh_commitments.frame_sequence,
        fresh_captured_at_unix_millis: fresh_commitments
            .source_frame
            .source_capture
            .captured_at_unix_millis,
        gameplay_authorization_valid_through_frame_sequence: sequence_commitments
            .gameplay_authorization_valid_through_frame_sequence,
    };
    let fresh_hwnd = fresh_manifest.pre.hwnd;
    let fresh_process_id = fresh_manifest.pre.process_id;
    let fresh_process_start_filetime_100ns = fresh_manifest.pre.process_start_filetime_100ns;
    let fresh_dpi = fresh_manifest.pre.dpi;
    let fresh_client_rect_desktop_px = fresh_manifest.pre.client_rect_desktop_px;
    let output_bounds = fresh_manifest.output.bounds_desktop_px;
    let (park_x_desktop_px, park_y_desktop_px) =
        choose_cursor_park_point_v3(&fresh_client_rect_desktop_px, &output_bounds)?;
    Ok(OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1 {
        _sequence: sequence,
        _fresh_perception: fresh_perception,
        _fresh_binding: fresh_binding,
        _before_input_postcondition: before_input_postcondition,
        commitments,
        primitive,
        target_points_desktop_px: resolved_targets.points_desktop_px,
        target_regions: resolved_targets.regions,
        hwnd: fresh_hwnd,
        process_id: fresh_process_id,
        process_start_filetime_100ns: fresh_process_start_filetime_100ns,
        dpi: fresh_dpi,
        client_rect_desktop_px: fresh_client_rect_desktop_px,
        park_x_desktop_px,
        park_y_desktop_px,
    })
}

/// Advances a coordinate-private gesture observation chain by exactly one
/// stage. The next target set must be on a strictly newer same-decision frame,
/// and the transition probe must identify one same-rectangle pixel change
/// tied to either the prior or next stage target. No input is sent, and the
/// resulting observation does not prove that an action caused the change.
pub fn advance_opaque_competitive_duel_gesture_sequence_v1(
    sequence: OpaqueMtgoCompetitiveDuelGestureSequenceV1,
    current_perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    target_set: MtgoVisibleDuelGestureTargetSetV1,
    transition_probe: MtgoCompetitiveDuelGestureVisibleTransitionProbeV1,
) -> Result<OpaqueMtgoCompetitiveDuelGestureSequenceV1, String> {
    let prior_sequence = sequence.commitments;
    let expected_stage_index = prior_sequence
        .current_stage_index
        .checked_add(1)
        .ok_or("gesture stage index overflow")?;
    if expected_stage_index >= prior_sequence.gesture_stage_count
        || target_set.stage_index != expected_stage_index
    {
        return Err("gesture sequence must advance by exactly one declared stage".to_owned());
    }
    let OpaqueMtgoCompetitiveDuelGestureStageV1 {
        _plan: plan,
        _continuation_frame: prior_continuation,
        _binding: prior_binding,
        commitments: prior_stage,
        target_points_desktop_px: prior_points,
        target_regions: prior_target_regions,
    } = sequence.current_stage;
    let next_required_roles = required_duel_gesture_target_roles_v1(
        &plan
            .gesture
            .stages_v1()
            .get(usize::from(expected_stage_index))
            .ok_or("gesture sequence names an absent next stage")?
            .primitive,
    );
    let before_perception = prior_continuation
        .as_deref()
        .unwrap_or(&plan.control.selection.perception);
    let before_manifest = &before_perception.source_frame.source_frame.manifest;
    let after_manifest = &current_perception.source_frame.source_frame.manifest;
    validate_same_duel_window_incarnation_v1(before_manifest, after_manifest)?;
    let before_capture = before_perception.commitments_v1();
    let after_capture = current_perception.commitments_v1();
    if target_set.frame_sequence <= prior_stage.frame_sequence
        || target_set.frame_id == prior_stage.frame_id
        || after_capture
            .source_frame
            .source_capture
            .captured_at_unix_millis
            <= before_capture
                .source_frame
                .source_capture
                .captured_at_unix_millis
        || after_capture
            .source_frame
            .source_capture
            .capture_commitment_sha256
            == before_capture
                .source_frame
                .source_capture
                .capture_commitment_sha256
    {
        return Err("gesture sequence continuation is not newer than its prior stage".to_owned());
    }
    let visible_transition_commitment_sha256 = validate_opaque_gesture_visible_transition_probe_v1(
        &transition_probe,
        &prior_stage,
        &prior_target_regions,
        before_perception,
        &current_perception,
        &target_set,
        &next_required_roles,
    )?;
    drop(prior_binding);
    drop(prior_points);
    drop(prior_continuation);
    let next_stage = bind_opaque_competitive_duel_gesture_stage_inner_v1(
        plan,
        Some(current_perception),
        target_set,
    )?;
    let next = next_stage.commitments_v1();
    let observed_stage_count = prior_sequence
        .observed_stage_count
        .checked_add(1)
        .ok_or("gesture observed-stage count overflow")?;
    let sequence_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_COMPETITIVE_GESTURE_SEQUENCE_DOMAIN_V1,
        &[
            prior_sequence.sequence_commitment_sha256.as_bytes(),
            visible_transition_commitment_sha256.as_bytes(),
            next.gesture_stage_binding_commitment_sha256.as_bytes(),
            next.opaque_gesture_stage_commitment_sha256.as_bytes(),
            &next.stage_index.to_be_bytes(),
            &observed_stage_count.to_be_bytes(),
            b"one_strictly_newer_visible_stage_no_input_or_action_causality",
        ],
    );
    Ok(OpaqueMtgoCompetitiveDuelGestureSequenceV1 {
        current_stage: next_stage,
        commitments: MtgoOpaqueCompetitiveDuelGestureSequenceCommitmentsV1 {
            competitive_action_plan_commitment_sha256: next
                .competitive_action_plan_commitment_sha256,
            gesture_plan_commitment_sha256: next.gesture_plan_commitment_sha256,
            competitive_mode_authorization_commitment_sha256: prior_sequence
                .competitive_mode_authorization_commitment_sha256,
            competitive_match_gameplay_authorization_commitment_sha256: prior_sequence
                .competitive_match_gameplay_authorization_commitment_sha256,
            policy_deployment_commitment_sha256: prior_sequence.policy_deployment_commitment_sha256,
            current_stage_binding_commitment_sha256: next.gesture_stage_binding_commitment_sha256,
            current_opaque_stage_commitment_sha256: next.opaque_gesture_stage_commitment_sha256,
            last_visible_transition_commitment_sha256: Some(visible_transition_commitment_sha256),
            sequence_commitment_sha256,
            selected_action_family: prior_sequence.selected_action_family,
            event_kind: prior_sequence.event_kind,
            game_number: prior_sequence.game_number,
            gameplay_authorization_valid_through_frame_sequence: prior_sequence
                .gameplay_authorization_valid_through_frame_sequence,
            current_stage_index: next.stage_index,
            gesture_stage_count: prior_sequence.gesture_stage_count,
            observed_stage_count,
            current_frame_id: next.frame_id,
            current_frame_sequence: next.frame_sequence,
        },
    })
}

/// Advances exactly one intermediate gesture stage using targets from the
/// exact profile-pinned runtime. The transition probe is derived internally
/// from changed same-rectangle evidence tied to the adjacent stages, so the
/// caller cannot choose either target coordinates or transition evidence.
/// This remains observation-only and grants no input authority.
pub fn advance_opaque_competitive_duel_gesture_sequence_from_pinned_runtime_v1(
    sequence: OpaqueMtgoCompetitiveDuelGestureSequenceV1,
    current_perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    profile: &AdmittedMtgoDuelGestureProfileV1,
    runtime: &OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoPinnedCompetitiveDuelGestureContinuationV1, String> {
    let prior = sequence.commitments_v1();
    let next_stage_index = prior
        .current_stage_index
        .checked_add(1)
        .ok_or("gesture continuation stage index overflow")?;
    if next_stage_index >= prior.gesture_stage_count {
        return Err("gesture continuation requires a declared later stage".to_owned());
    }
    let before_perception = sequence
        .current_stage
        ._continuation_frame
        .as_deref()
        .unwrap_or(&sequence.current_stage._plan.control.selection.perception);
    validate_same_duel_window_incarnation_v1(
        &before_perception.source_frame.source_frame.manifest,
        &current_perception.source_frame.source_frame.manifest,
    )?;
    let before_capture = before_perception.commitments_v1();
    let after_capture = current_perception.commitments_v1();
    if after_capture.frame_sequence <= prior.current_frame_sequence
        || after_capture.frame_id == prior.current_frame_id
        || after_capture.frame_sequence > prior.gameplay_authorization_valid_through_frame_sequence
        || after_capture
            .source_frame
            .source_capture
            .captured_at_unix_millis
            <= before_capture
                .source_frame
                .source_capture
                .captured_at_unix_millis
        || after_capture
            .source_frame
            .source_capture
            .capture_commitment_sha256
            == before_capture
                .source_frame
                .source_capture
                .capture_commitment_sha256
        || current_perception.runtime_identity_commitment_sha256
            != before_perception.runtime_identity_commitment_sha256
        || current_perception
            .source_frame
            .perception_profile_admission_commitment_sha256
            != before_perception
                .source_frame
                .perception_profile_admission_commitment_sha256
        || current_perception.decision_record.payload
            != sequence
                .current_stage
                ._plan
                .control
                .selection
                .perception
                .decision_record
                .payload
    {
        return Err(
            "pinned gesture continuation is not an exact newer authorized decision frame"
                .to_owned(),
        );
    }
    let (target_set, target_runtime) = invoke_pinned_gesture_target_runtime_for_stage_v1(
        &sequence,
        &current_perception,
        profile,
        runtime,
        next_stage_index,
        timeout_ms,
    )?;
    let transition_probe = derive_opaque_gesture_visible_transition_probe_v1(
        &sequence,
        &current_perception,
        &target_set,
    )?;
    let advanced = advance_opaque_competitive_duel_gesture_sequence_v1(
        sequence,
        current_perception,
        target_set,
        transition_probe,
    )?;
    let next = advanced.commitments_v1();
    let visible_transition_commitment_sha256 = next
        .last_visible_transition_commitment_sha256
        .clone()
        .ok_or("pinned gesture continuation lost its visible transition")?;
    if next.current_stage_index != next_stage_index
        || next.observed_stage_count != next_stage_index.saturating_add(1)
        || next.current_frame_sequence <= prior.current_frame_sequence
    {
        return Err("pinned gesture continuation did not advance exactly one stage".to_owned());
    }
    let family_json = serde_json::to_vec(&next.selected_action_family)
        .map_err(|error| format!("serialize pinned continuation family: {error}"))?;
    let event_kind_json = serde_json::to_vec(&next.event_kind)
        .map_err(|error| format!("serialize pinned continuation event kind: {error}"))?;
    let continuation_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_PINNED_COMPETITIVE_GESTURE_CONTINUATION_DOMAIN_V1,
        &[
            prior.sequence_commitment_sha256.as_bytes(),
            next.sequence_commitment_sha256.as_bytes(),
            visible_transition_commitment_sha256.as_bytes(),
            target_runtime.runtime_identity_commitment_sha256.as_bytes(),
            target_runtime.request_commitment_sha256.as_bytes(),
            &family_json,
            &event_kind_json,
            &[next.game_number],
            &next.current_stage_index.to_be_bytes(),
            &next.gesture_stage_count.to_be_bytes(),
            &next.current_frame_id.to_be_bytes(),
            &next.current_frame_sequence.to_be_bytes(),
            &advanced
                .current_stage
                .commitments
                .target_count
                .to_be_bytes(),
            b"one_runtime_pinned_adjacent_visible_stage_no_input_or_action_causality",
        ],
    );
    let commitments = MtgoOpaquePinnedCompetitiveDuelGestureContinuationCommitmentsV1 {
        prior_sequence_commitment_sha256: prior.sequence_commitment_sha256,
        advanced_sequence_commitment_sha256: next.sequence_commitment_sha256,
        visible_transition_commitment_sha256,
        gesture_target_runtime_identity_commitment_sha256: target_runtime
            .runtime_identity_commitment_sha256,
        gesture_target_request_commitment_sha256: target_runtime.request_commitment_sha256,
        continuation_commitment_sha256,
        selected_action_family: next.selected_action_family,
        event_kind: next.event_kind,
        game_number: next.game_number,
        stage_index: next.current_stage_index,
        gesture_stage_count: next.gesture_stage_count,
        frame_id: next.current_frame_id,
        frame_sequence: next.current_frame_sequence,
        captured_at_unix_millis: after_capture
            .source_frame
            .source_capture
            .captured_at_unix_millis,
        target_count: advanced.current_stage.commitments.target_count,
    };
    Ok(OpaqueMtgoPinnedCompetitiveDuelGestureContinuationV1 {
        _sequence: advanced,
        commitments,
    })
}

/// Consumes the exact primitive preparation that preceded input, promotes its
/// retained fresh frame to the sequence baseline, and observes one declared
/// later stage through the pinned target runtime. This function sends no input
/// and does not itself claim that input caused the visible transition.
pub(crate) fn advance_prepared_competitive_duel_gesture_sequence_from_pinned_runtime_v1(
    prepared: OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1,
    current_perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    profile: &AdmittedMtgoDuelGestureProfileV1,
    runtime: &OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoPinnedCompetitiveDuelGestureContinuationV1, String> {
    let prepared_commitments = prepared.commitments.clone();
    if prepared_commitments
        .stage_index
        .checked_add(1)
        .ok_or("prepared gesture continuation stage index overflow")?
        >= prepared_commitments.gesture_stage_count
    {
        return Err("prepared gesture primitive has no declared continuation stage".to_owned());
    }
    let current = current_perception.commitments_v1();
    if current.frame_sequence <= prepared_commitments.fresh_frame_sequence
        || current.frame_id == prepared_commitments.fresh_frame_id
        || current.source_frame.source_capture.captured_at_unix_millis
            <= prepared_commitments.fresh_captured_at_unix_millis
    {
        return Err(
            "gesture continuation is not newer than its prepared input baseline".to_owned(),
        );
    }
    let sequence = promote_prepared_gesture_sequence_baseline_v1(prepared)?;
    let promoted = sequence.commitments_v1();
    if promoted.sequence_commitment_sha256
        != prepared_commitments.prepared_sequence_commitment_sha256
        || promoted.current_stage_index != prepared_commitments.stage_index
        || promoted.current_frame_id != prepared_commitments.fresh_frame_id
        || promoted.current_frame_sequence != prepared_commitments.fresh_frame_sequence
    {
        return Err("prepared gesture sequence baseline promotion changed its identity".to_owned());
    }
    let continuation = advance_opaque_competitive_duel_gesture_sequence_from_pinned_runtime_v1(
        sequence,
        current_perception,
        profile,
        runtime,
        timeout_ms,
    )?;
    if continuation.commitments.prior_sequence_commitment_sha256
        != prepared_commitments.prepared_sequence_commitment_sha256
    {
        return Err("pinned continuation did not consume the exact prepared sequence".to_owned());
    }
    Ok(continuation)
}

/// Rechecks the already observed next stage on one additional fresh frame.
/// This separates visible transition confirmation from target preparation, so
/// the transition frame itself can never be routed directly to another input.
pub(crate) fn prepare_opaque_competitive_duel_gesture_continuation_stage_from_pinned_runtime_v1(
    continuation: OpaqueMtgoPinnedCompetitiveDuelGestureContinuationV1,
    fresh_perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    profile: &AdmittedMtgoDuelGestureProfileV1,
    runtime: &OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1, String> {
    let OpaqueMtgoPinnedCompetitiveDuelGestureContinuationV1 {
        _sequence: sequence,
        commitments: continuation_commitments,
    } = continuation;
    let sequence_commitments = sequence.commitments_v1();
    if continuation_commitments.advanced_sequence_commitment_sha256
        != sequence_commitments.sequence_commitment_sha256
        || continuation_commitments.stage_index != sequence_commitments.current_stage_index
        || continuation_commitments.gesture_stage_count != sequence_commitments.gesture_stage_count
        || continuation_commitments.frame_id != sequence_commitments.current_frame_id
        || continuation_commitments.frame_sequence != sequence_commitments.current_frame_sequence
        || continuation_commitments.selected_action_family
            != sequence_commitments.selected_action_family
        || continuation_commitments.event_kind != sequence_commitments.event_kind
        || continuation_commitments.game_number != sequence_commitments.game_number
        || continuation_commitments.target_count != sequence.current_stage.commitments.target_count
        || sequence_commitments.current_stage_index == 0
        || sequence_commitments.current_stage_index >= sequence_commitments.gesture_stage_count
    {
        return Err("pinned continuation and retained gesture sequence differ".to_owned());
    }
    prepare_opaque_competitive_duel_gesture_current_stage_from_pinned_runtime_v1(
        sequence,
        fresh_perception,
        profile,
        runtime,
        timeout_ms,
    )
}

fn promote_prepared_gesture_sequence_baseline_v1(
    prepared: OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1,
) -> Result<OpaqueMtgoCompetitiveDuelGestureSequenceV1, String> {
    let prepared_commitments = prepared.commitments.clone();
    let OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1 {
        _sequence: sequence,
        _fresh_perception: fresh_perception,
        _fresh_binding: fresh_binding,
        _before_input_postcondition: _,
        commitments: _,
        primitive,
        target_points_desktop_px,
        target_regions,
        ..
    } = prepared;
    let OpaqueMtgoCompetitiveDuelGestureSequenceV1 {
        current_stage,
        commitments: mut sequence_commitments,
    } = sequence;
    if sequence_commitments.sequence_commitment_sha256
        != prepared_commitments.source_sequence_commitment_sha256
        || sequence_commitments.current_stage_index != prepared_commitments.stage_index
        || sequence_commitments.gesture_stage_count != prepared_commitments.gesture_stage_count
    {
        return Err("prepared gesture baseline differs from its source sequence".to_owned());
    }
    let OpaqueMtgoCompetitiveDuelGestureStageV1 {
        _plan: plan,
        commitments: source_stage_commitments,
        ..
    } = current_stage;
    let plan_commitments = plan.commitments_v1();
    let fresh_binding_commitments = fresh_binding.commitments_v1();
    let fresh_perception_commitments = fresh_perception.commitments_v1();
    if fresh_binding_commitments.stage_index != prepared_commitments.stage_index
        || fresh_binding_commitments.frame_id != prepared_commitments.fresh_frame_id
        || fresh_binding_commitments.frame_sequence != prepared_commitments.fresh_frame_sequence
        || fresh_binding_commitments.binding_commitment_sha256
            != prepared_commitments.fresh_stage_binding_commitment_sha256
        || fresh_perception_commitments.frame_id != prepared_commitments.fresh_frame_id
        || fresh_perception_commitments.frame_sequence != prepared_commitments.fresh_frame_sequence
        || fresh_perception_commitments
            .source_frame
            .source_capture
            .capture_commitment_sha256
            != prepared_commitments.fresh_capture_commitment_sha256
        || fresh_perception_commitments.perception_result_commitment_sha256
            != prepared_commitments.fresh_perception_result_commitment_sha256
    {
        return Err("prepared gesture baseline retained inconsistent fresh-frame state".to_owned());
    }
    let primitive_json = serde_json::to_vec(&primitive)
        .map_err(|error| format!("serialize promoted gesture primitive: {error}"))?;
    let target_points_json = serde_json::to_vec(&target_points_desktop_px)
        .map_err(|error| format!("serialize promoted gesture target points: {error}"))?;
    let opaque_gesture_stage_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_COMPETITIVE_GESTURE_STAGE_DOMAIN_V1,
        &[
            plan_commitments
                .opaque_competitive_action_plan_commitment_sha256
                .as_bytes(),
            plan_commitments.gesture_plan_commitment_sha256.as_bytes(),
            fresh_binding_commitments
                .binding_commitment_sha256
                .as_bytes(),
            source_stage_commitments
                .source_perception_result_commitment_sha256
                .as_bytes(),
            fresh_perception_commitments
                .perception_result_commitment_sha256
                .as_bytes(),
            fresh_perception_commitments
                .source_frame
                .source_capture
                .capture_commitment_sha256
                .as_bytes(),
            b"continuation",
            &primitive_json,
            &target_points_json,
            b"private_pixel_rehashed_target_points_no_input_or_event_entry_authority",
        ],
    );
    let promoted_stage_commitments = MtgoOpaqueCompetitiveDuelGestureStageCommitmentsV1 {
        competitive_action_plan_commitment_sha256: plan_commitments
            .opaque_competitive_action_plan_commitment_sha256,
        gesture_plan_commitment_sha256: plan_commitments.gesture_plan_commitment_sha256,
        gesture_stage_binding_commitment_sha256: fresh_binding_commitments
            .binding_commitment_sha256,
        source_perception_result_commitment_sha256: source_stage_commitments
            .source_perception_result_commitment_sha256,
        bound_perception_result_commitment_sha256: fresh_perception_commitments
            .perception_result_commitment_sha256,
        bound_capture_commitment_sha256: fresh_perception_commitments
            .source_frame
            .source_capture
            .capture_commitment_sha256,
        opaque_gesture_stage_commitment_sha256,
        selected_action_family: prepared_commitments.selected_action_family,
        stage_index: prepared_commitments.stage_index,
        frame_id: prepared_commitments.fresh_frame_id,
        frame_sequence: prepared_commitments.fresh_frame_sequence,
        target_count: prepared_commitments.target_count,
    };
    let expected_prepared_sequence_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_COMPETITIVE_GESTURE_SEQUENCE_DOMAIN_V1,
        &[
            sequence_commitments.sequence_commitment_sha256.as_bytes(),
            prepared_commitments
                .fresh_stage_binding_commitment_sha256
                .as_bytes(),
            prepared_commitments
                .fresh_capture_commitment_sha256
                .as_bytes(),
            prepared_commitments
                .fresh_perception_result_commitment_sha256
                .as_bytes(),
            prepared_commitments.primitive_commitment_sha256.as_bytes(),
            prepared_commitments
                .preparation_commitment_sha256
                .as_bytes(),
            &prepared_commitments.fresh_frame_id.to_be_bytes(),
            &prepared_commitments.fresh_frame_sequence.to_be_bytes(),
            b"same_stage_refreshed_immediately_before_input_no_action_causality",
        ],
    );
    if expected_prepared_sequence_commitment_sha256
        != prepared_commitments.prepared_sequence_commitment_sha256
    {
        return Err("prepared gesture sequence commitment did not recompute".to_owned());
    }
    sequence_commitments.current_stage_binding_commitment_sha256 = promoted_stage_commitments
        .gesture_stage_binding_commitment_sha256
        .clone();
    sequence_commitments.current_opaque_stage_commitment_sha256 = promoted_stage_commitments
        .opaque_gesture_stage_commitment_sha256
        .clone();
    sequence_commitments.sequence_commitment_sha256 =
        prepared_commitments.prepared_sequence_commitment_sha256;
    sequence_commitments.current_frame_id = prepared_commitments.fresh_frame_id;
    sequence_commitments.current_frame_sequence = prepared_commitments.fresh_frame_sequence;
    Ok(OpaqueMtgoCompetitiveDuelGestureSequenceV1 {
        current_stage: OpaqueMtgoCompetitiveDuelGestureStageV1 {
            _plan: plan,
            _continuation_frame: Some(Box::new(fresh_perception)),
            _binding: fresh_binding,
            commitments: promoted_stage_commitments,
            target_points_desktop_px,
            target_regions,
        },
        commitments: sequence_commitments,
    })
}

fn invoke_pinned_gesture_target_runtime_for_stage_v1(
    sequence: &OpaqueMtgoCompetitiveDuelGestureSequenceV1,
    perception: &OpaqueMtgoAdmittedDuelPerceptionV1,
    profile: &AdmittedMtgoDuelGestureProfileV1,
    runtime: &OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1,
    stage_index: u16,
    timeout_ms: u32,
) -> Result<
    (
        MtgoVisibleDuelGestureTargetSetV1,
        VerifiedDuelGestureTargetRuntimeBindingV1,
    ),
    String,
> {
    invoke_pinned_gesture_target_runtime_for_plan_stage_v1(
        &sequence.current_stage._plan,
        perception,
        profile,
        runtime,
        stage_index,
        timeout_ms,
    )
}

fn invoke_pinned_gesture_target_runtime_for_plan_stage_v1(
    plan: &OpaqueMtgoCompetitiveDuelActionPlanV1,
    perception: &OpaqueMtgoAdmittedDuelPerceptionV1,
    profile: &AdmittedMtgoDuelGestureProfileV1,
    runtime: &OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1,
    stage_index: u16,
    timeout_ms: u32,
) -> Result<
    (
        MtgoVisibleDuelGestureTargetSetV1,
        VerifiedDuelGestureTargetRuntimeBindingV1,
    ),
    String,
> {
    if !(100..=60_000).contains(&timeout_ms) {
        return Err("duel gesture-target timeout must be between 100 and 60000 ms".to_owned());
    }
    let plan_commitments = plan.commitments_v1();
    let stage = plan
        .gesture
        .stages_v1()
        .get(usize::from(stage_index))
        .ok_or("gesture-target runtime stage is absent from the exact plan")?;
    if stage.stage_index != stage_index
        || stage_index >= plan_commitments.gesture_stage_count
        || profile.supported_action_families() != canonical_duel_gesture_action_families_v1()
        || !profile
            .supported_action_families()
            .contains(&plan.control.selected_action_family)
        || runtime.commitments.gesture_evaluation_commitment_sha256
            != profile.evaluation_commitment_sha256()
        || runtime
            .commitments
            .gesture_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
        || runtime
            .commitments
            .perception_profile_admission_commitment_sha256
            != profile.perception_profile_admission_commitment_sha256()
        || perception
            .source_frame
            .perception_profile_admission_commitment_sha256
            != profile.perception_profile_admission_commitment_sha256()
    {
        return Err("gesture stage, runtime, and admitted all-family profile differ".to_owned());
    }
    verify_gesture_target_runtime_identity_now_v1(runtime)?;

    let perception_commitments = perception.commitments_v1();
    let source = &perception.source_frame.source_frame;
    let width = source.manifest.frame.canonical_width;
    let height = source.manifest.frame.canonical_height;
    let stride = width
        .checked_mul(4)
        .ok_or("duel gesture-target canonical stride overflow")?;
    let header = MtgoDuelGestureTargetRequestHeaderV1 {
        schema_version: 1,
        protocol: "mtgo_visible_duel_gesture_target_v1".to_owned(),
        frame_id: perception_commitments.frame_id,
        frame_sequence: perception_commitments.frame_sequence,
        canonical_width: width,
        canonical_height: height,
        canonical_stride: stride,
        canonical_byte_length: source.canonical_bgra8.len(),
        canonical_bgra8_sha256: source.manifest.frame.canonical_bgra8_sha256.clone(),
        source_capture_commitment_sha256: perception_commitments
            .source_frame
            .source_capture
            .capture_commitment_sha256,
        perception_result_commitment_sha256: perception_commitments
            .perception_result_commitment_sha256,
        decision_commitment_sha256: perception_commitments.decision_commitment_sha256,
        gesture_plan_commitment_sha256: plan_commitments.gesture_plan_commitment_sha256,
        selected_action_family: plan.control.selected_action_family,
        stage_index,
        primitive: stage.primitive.clone(),
        gesture_evaluation_commitment_sha256: profile.evaluation_commitment_sha256().to_owned(),
        gesture_profile_admission_commitment_sha256: profile
            .admission_commitment_sha256()
            .to_owned(),
        runtime_identity_commitment_sha256: runtime
            .commitments
            .runtime_identity_commitment_sha256
            .clone(),
        gesture_target_runtime_binary_sha256: profile
            .gesture_target_runtime_binary_sha256()
            .to_owned(),
        gesture_target_assets_manifest_sha256: profile
            .gesture_target_assets_manifest_sha256()
            .to_owned(),
    };
    let header_json = serde_json::to_vec(&header)
        .map_err(|error| format!("serialize duel gesture-target request: {error}"))?;
    let checked_request =
        check_untrusted_duel_gesture_target_request_v1(&header_json, &source.canonical_bgra8)?;
    let request_commitment_sha256 = checked_request.request_commitment_sha256_v1().to_owned();
    let response = invoke_verified_gesture_target_process_v1(
        runtime,
        &header_json,
        &source.canonical_bgra8,
        Duration::from_millis(u64::from(timeout_ms)),
    )?;
    verify_gesture_target_runtime_identity_now_v1(runtime)?;
    let response: MtgoDuelGestureTargetProcessResponseV1 = serde_json::from_slice(&response)
        .map_err(|error| {
            format!("gesture-target response is not one strict protocol JSON value: {error}")
        })?;
    if response.schema_version != 1
        || response.request_commitment_sha256 != request_commitment_sha256
        || response.target_set.stage_index != stage_index
    {
        return Err("gesture-target response does not bind the exact stage request".to_owned());
    }
    Ok((
        response.target_set,
        VerifiedDuelGestureTargetRuntimeBindingV1 {
            runtime_identity_commitment_sha256: runtime
                .commitments
                .runtime_identity_commitment_sha256
                .clone(),
            request_commitment_sha256,
        },
    ))
}

fn derive_opaque_gesture_visible_transition_probe_v1(
    sequence: &OpaqueMtgoCompetitiveDuelGestureSequenceV1,
    current_perception: &OpaqueMtgoAdmittedDuelPerceptionV1,
    target_set: &MtgoVisibleDuelGestureTargetSetV1,
) -> Result<MtgoCompetitiveDuelGestureVisibleTransitionProbeV1, String> {
    let prior = sequence.commitments_v1();
    let expected_stage_index = prior
        .current_stage_index
        .checked_add(1)
        .ok_or("gesture transition stage index overflow")?;
    if target_set.stage_index != expected_stage_index
        || target_set.frame_id != current_perception.commitments_v1().frame_id
        || target_set.frame_sequence != current_perception.commitments_v1().frame_sequence
    {
        return Err("gesture transition target set does not bind the exact next frame".to_owned());
    }
    let before_perception = sequence
        .current_stage
        ._continuation_frame
        .as_deref()
        .unwrap_or(&sequence.current_stage._plan.control.selection.perception);
    let before_size = MtgoSizePxV1 {
        width: before_perception
            .source_frame
            .source_frame
            .manifest
            .frame
            .canonical_width,
        height: before_perception
            .source_frame
            .source_frame
            .manifest
            .frame
            .canonical_height,
    };
    let after_size = MtgoSizePxV1 {
        width: current_perception
            .source_frame
            .source_frame
            .manifest
            .frame
            .canonical_width,
        height: current_perception
            .source_frame
            .source_frame
            .manifest
            .frame
            .canonical_height,
    };
    if before_size != after_size {
        return Err("gesture transition frames changed client size".to_owned());
    }

    let mut candidate_pairs = Vec::new();
    for prior_target in &sequence.current_stage.target_regions {
        for evidence in &current_perception.decision_record.evidence {
            if let MtgoEvidenceSourceV1::FrameRegion { frame_id, rect, .. } = &evidence.source {
                if *frame_id == target_set.frame_id && rect == &prior_target.rect_client_px {
                    candidate_pairs
                        .push((prior_target.frame_region_evidence_id, evidence.evidence_id));
                }
            }
        }
    }
    for next_target in &target_set.targets {
        let (next_rect, _) = frame_region_for_evidence_v1(
            &current_perception.decision_record,
            next_target.frame_region_evidence_id,
        )?;
        for evidence in &before_perception.decision_record.evidence {
            if let MtgoEvidenceSourceV1::FrameRegion { frame_id, rect, .. } = &evidence.source {
                if *frame_id == prior.current_frame_id && rect == next_rect {
                    candidate_pairs
                        .push((evidence.evidence_id, next_target.frame_region_evidence_id));
                }
            }
        }
    }
    candidate_pairs.sort_unstable();
    candidate_pairs.dedup();

    for (before_evidence_id, after_evidence_id) in candidate_pairs {
        let (before_rect, before_recorded_sha256) =
            frame_region_for_evidence_v1(&before_perception.decision_record, before_evidence_id)?;
        let (after_rect, after_recorded_sha256) =
            frame_region_for_evidence_v1(&current_perception.decision_record, after_evidence_id)?;
        if before_rect != after_rect {
            continue;
        }
        let before_actual_sha256 = visible_frame_region_content_sha256_v1(
            &before_perception.source_frame.source_frame.canonical_bgra8,
            &before_size,
            before_rect,
        )
        .map_err(|error| format!("rehash derived gesture transition before region: {error}"))?;
        let after_actual_sha256 = visible_frame_region_content_sha256_v1(
            &current_perception.source_frame.source_frame.canonical_bgra8,
            &after_size,
            after_rect,
        )
        .map_err(|error| format!("rehash derived gesture transition after region: {error}"))?;
        if before_actual_sha256 != before_recorded_sha256
            || after_actual_sha256 != after_recorded_sha256
        {
            return Err(
                "derived gesture transition evidence differs from retained pixels".to_owned(),
            );
        }
        if before_actual_sha256 != after_actual_sha256 {
            return Ok(MtgoCompetitiveDuelGestureVisibleTransitionProbeV1 {
                schema_version: 1,
                gesture_plan_commitment_sha256: prior.gesture_plan_commitment_sha256,
                from_stage_binding_commitment_sha256: prior.current_stage_binding_commitment_sha256,
                from_stage_index: prior.current_stage_index,
                to_stage_index: expected_stage_index,
                before_frame_id: prior.current_frame_id,
                before_frame_sequence: prior.current_frame_sequence,
                after_frame_id: target_set.frame_id,
                after_frame_sequence: target_set.frame_sequence,
                before_frame_region_evidence_id: before_evidence_id,
                after_frame_region_evidence_id: after_evidence_id,
            });
        }
    }
    Err("no changed same-rectangle evidence ties the adjacent gesture stages".to_owned())
}

fn bind_opaque_competitive_duel_gesture_stage_inner_v1(
    plan: OpaqueMtgoCompetitiveDuelActionPlanV1,
    current_perception: Option<OpaqueMtgoAdmittedDuelPerceptionV1>,
    target_set: MtgoVisibleDuelGestureTargetSetV1,
) -> Result<OpaqueMtgoCompetitiveDuelGestureStageV1, String> {
    let plan_commitments = plan.commitments_v1();
    let source_perception = &plan.control.selection.perception;
    let source_perception_commitments = source_perception.commitments_v1();
    let (bound_perception, frame_kind) = match &current_perception {
        None => (source_perception, b"source".as_slice()),
        Some(current) => {
            let source_manifest = &source_perception.source_frame.source_frame.manifest;
            let current_manifest = &current.source_frame.source_frame.manifest;
            validate_same_duel_window_incarnation_v1(source_manifest, current_manifest)?;
            let source_capture = source_perception.source_frame.commitments_v1();
            let current_capture = current.source_frame.commitments_v1();
            if current_capture.source_capture.capture_commitment_sha256
                == source_capture.source_capture.capture_commitment_sha256
                || current_capture.source_capture.captured_at_unix_millis
                    <= source_capture.source_capture.captured_at_unix_millis
                || current.commitments_v1().frame_sequence <= plan_commitments.frame_sequence
                || current.commitments_v1().frame_id == plan_commitments.frame_id
            {
                return Err(
                    "gesture continuation is not a distinct strictly newer opaque frame".to_owned(),
                );
            }
            if current.decision_record.payload != source_perception.decision_record.payload {
                return Err(
                    "gesture continuation changed the selected duel decision payload".to_owned(),
                );
            }
            if current.runtime_identity_commitment_sha256
                != source_perception.runtime_identity_commitment_sha256
                || current.source_frame.perception_profile_commitment_sha256
                    != source_perception
                        .source_frame
                        .perception_profile_commitment_sha256
                || current
                    .source_frame
                    .perception_profile_admission_commitment_sha256
                    != source_perception
                        .source_frame
                        .perception_profile_admission_commitment_sha256
            {
                return Err(
                    "gesture continuation changed the perception profile or runtime".to_owned(),
                );
            }
            (current, b"continuation".as_slice())
        }
    };
    if target_set.frame_sequence
        > plan_commitments.gameplay_authorization_valid_through_frame_sequence
    {
        return Err("gesture stage exceeds the gameplay authorization lifetime".to_owned());
    }
    let stage = plan
        .gesture
        .stages_v1()
        .get(usize::from(target_set.stage_index))
        .ok_or("gesture target set names an unknown stage")?;
    let resolved_targets = resolve_opaque_gesture_target_points_v1(
        &target_set,
        &stage.primitive,
        bound_perception,
        if target_set.stage_index == 0 {
            Some((
                &plan.control.rect_client_px,
                &plan.control.selected_region_content_sha256,
            ))
        } else {
            None
        },
    )?;
    let binding = bind_visible_duel_gesture_stage_v1(
        &plan.gesture,
        &bound_perception.validated_decision,
        target_set,
    )
    .map_err(|error| format!("bind opaque duel gesture stage: {error}"))?;
    let binding_commitments = binding.commitments_v1();
    let bound_perception_commitments = bound_perception.commitments_v1();
    let primitive_json = serde_json::to_vec(&stage.primitive)
        .map_err(|error| format!("serialize duel gesture primitive: {error}"))?;
    let target_points_json = serde_json::to_vec(&resolved_targets.points_desktop_px)
        .map_err(|error| format!("serialize private duel gesture target points: {error}"))?;
    let opaque_gesture_stage_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_COMPETITIVE_GESTURE_STAGE_DOMAIN_V1,
        &[
            plan_commitments
                .opaque_competitive_action_plan_commitment_sha256
                .as_bytes(),
            plan_commitments.gesture_plan_commitment_sha256.as_bytes(),
            binding_commitments.binding_commitment_sha256.as_bytes(),
            source_perception_commitments
                .perception_result_commitment_sha256
                .as_bytes(),
            bound_perception_commitments
                .perception_result_commitment_sha256
                .as_bytes(),
            bound_perception_commitments
                .source_frame
                .source_capture
                .capture_commitment_sha256
                .as_bytes(),
            frame_kind,
            &primitive_json,
            &target_points_json,
            b"private_pixel_rehashed_target_points_no_input_or_event_entry_authority",
        ],
    );
    let commitments = MtgoOpaqueCompetitiveDuelGestureStageCommitmentsV1 {
        competitive_action_plan_commitment_sha256: plan_commitments
            .opaque_competitive_action_plan_commitment_sha256,
        gesture_plan_commitment_sha256: plan_commitments.gesture_plan_commitment_sha256,
        gesture_stage_binding_commitment_sha256: binding_commitments.binding_commitment_sha256,
        source_perception_result_commitment_sha256: source_perception_commitments
            .perception_result_commitment_sha256,
        bound_perception_result_commitment_sha256: bound_perception_commitments
            .perception_result_commitment_sha256,
        bound_capture_commitment_sha256: bound_perception_commitments
            .source_frame
            .source_capture
            .capture_commitment_sha256,
        opaque_gesture_stage_commitment_sha256,
        selected_action_family: plan.control.selected_action_family,
        stage_index: binding_commitments.stage_index,
        frame_id: binding_commitments.frame_id,
        frame_sequence: binding_commitments.frame_sequence,
        target_count: binding_commitments.target_count,
    };
    Ok(OpaqueMtgoCompetitiveDuelGestureStageV1 {
        _plan: plan,
        _continuation_frame: current_perception.map(Box::new),
        _binding: binding,
        commitments,
        target_points_desktop_px: resolved_targets.points_desktop_px,
        target_regions: resolved_targets.regions,
    })
}

fn validate_opaque_gesture_visible_transition_probe_v1(
    probe: &MtgoCompetitiveDuelGestureVisibleTransitionProbeV1,
    prior_stage: &MtgoOpaqueCompetitiveDuelGestureStageCommitmentsV1,
    prior_target_regions: &[PrivateOpaqueGestureTargetRegionV1],
    before: &OpaqueMtgoAdmittedDuelPerceptionV1,
    after: &OpaqueMtgoAdmittedDuelPerceptionV1,
    next_target_set: &MtgoVisibleDuelGestureTargetSetV1,
    next_required_roles: &[MtgoDuelGestureTargetRoleV1],
) -> Result<String, String> {
    if probe.schema_version != MTGO_OPAQUE_COMPETITIVE_DUEL_GESTURE_TRANSITION_SCHEMA_V1
        || probe.gesture_plan_commitment_sha256 != prior_stage.gesture_plan_commitment_sha256
        || probe.from_stage_binding_commitment_sha256
            != prior_stage.gesture_stage_binding_commitment_sha256
        || probe.from_stage_index != prior_stage.stage_index
        || probe.to_stage_index != next_target_set.stage_index
        || probe.to_stage_index != probe.from_stage_index.saturating_add(1)
        || probe.before_frame_id != prior_stage.frame_id
        || probe.before_frame_sequence != prior_stage.frame_sequence
        || probe.after_frame_id != next_target_set.frame_id
        || probe.after_frame_sequence != next_target_set.frame_sequence
    {
        return Err(
            "gesture visible-transition probe does not bind adjacent exact stages".to_owned(),
        );
    }
    let before_is_prior_target = prior_target_regions
        .iter()
        .any(|target| target.frame_region_evidence_id == probe.before_frame_region_evidence_id);
    let after_is_next_target = next_target_set.targets.iter().any(|target| {
        target.frame_region_evidence_id == probe.after_frame_region_evidence_id
            && next_required_roles.contains(&target.role)
    });
    if !before_is_prior_target && !after_is_next_target {
        return Err(
            "gesture visible-transition probe is unrelated to both adjacent stages".to_owned(),
        );
    }
    let (before_rect, before_recorded_sha256) = frame_region_for_evidence_v1(
        &before.decision_record,
        probe.before_frame_region_evidence_id,
    )?;
    let (after_rect, after_recorded_sha256) =
        frame_region_for_evidence_v1(&after.decision_record, probe.after_frame_region_evidence_id)?;
    if before_rect != after_rect {
        return Err(
            "gesture visible-transition probe must compare the same client rectangle".to_owned(),
        );
    }
    let before_source = &before.source_frame.source_frame;
    let after_source = &after.source_frame.source_frame;
    let before_size = MtgoSizePxV1 {
        width: before_source.manifest.frame.canonical_width,
        height: before_source.manifest.frame.canonical_height,
    };
    let after_size = MtgoSizePxV1 {
        width: after_source.manifest.frame.canonical_width,
        height: after_source.manifest.frame.canonical_height,
    };
    if before_size != after_size {
        return Err("gesture visible-transition frames changed client size".to_owned());
    }
    let (before_actual_sha256, after_actual_sha256) = validate_changed_gesture_region_pixels_v1(
        &before_source.canonical_bgra8,
        &after_source.canonical_bgra8,
        &before_size,
        before_rect,
        before_recorded_sha256,
        after_recorded_sha256,
    )?;
    let probe_json = serde_json::to_vec(probe)
        .map_err(|error| format!("serialize gesture visible-transition probe: {error}"))?;
    let rect_json = serde_json::to_vec(before_rect)
        .map_err(|error| format!("serialize gesture visible-transition rectangle: {error}"))?;
    Ok(commitment_v1(
        DUEL_OPAQUE_COMPETITIVE_GESTURE_TRANSITION_DOMAIN_V1,
        &[
            &probe_json,
            &rect_json,
            before_actual_sha256.as_bytes(),
            after_actual_sha256.as_bytes(),
            &[u8::from(before_is_prior_target)],
            &[u8::from(after_is_next_target)],
            b"one_adjacent_stage_visible_region_changed_no_input_or_action_causality",
        ],
    ))
}

fn validate_changed_gesture_region_pixels_v1(
    before_pixels: &[u8],
    after_pixels: &[u8],
    size: &MtgoSizePxV1,
    rect: &MtgoRectPxV1,
    before_recorded_sha256: &str,
    after_recorded_sha256: &str,
) -> Result<(String, String), String> {
    let before_actual_sha256 = visible_frame_region_content_sha256_v1(before_pixels, size, rect)
        .map_err(|error| format!("rehash gesture transition before region: {error}"))?;
    let after_actual_sha256 = visible_frame_region_content_sha256_v1(after_pixels, size, rect)
        .map_err(|error| format!("rehash gesture transition after region: {error}"))?;
    if before_actual_sha256 != before_recorded_sha256
        || after_actual_sha256 != after_recorded_sha256
    {
        return Err(
            "gesture visible-transition evidence differs from retained opaque pixels".to_owned(),
        );
    }
    if before_actual_sha256 == after_actual_sha256 {
        return Err("gesture visible-transition probe region did not change".to_owned());
    }
    Ok((before_actual_sha256, after_actual_sha256))
}

fn resolve_opaque_gesture_target_points_v1(
    target_set: &MtgoVisibleDuelGestureTargetSetV1,
    primitive: &MtgoDuelGesturePrimitiveV1,
    perception: &OpaqueMtgoAdmittedDuelPerceptionV1,
    exact_primary: Option<(&MtgoRectPxV1, &str)>,
) -> Result<ResolvedOpaqueGestureTargetsV1, String> {
    let source = &perception.source_frame.source_frame;
    let size = MtgoSizePxV1 {
        width: source.manifest.frame.canonical_width,
        height: source.manifest.frame.canonical_height,
    };
    let client_rect = source.manifest.pre.client_rect_desktop_px;
    let required_roles = required_duel_gesture_target_roles_v1(primitive);
    let mut points = Vec::with_capacity(required_roles.len());
    let mut regions = Vec::with_capacity(required_roles.len());
    for role in required_roles {
        let matches: Vec<_> = target_set
            .targets
            .iter()
            .filter(|target| target.role == role)
            .collect();
        if matches.len() != 1 {
            return Err("gesture stage target role is absent or ambiguous".to_owned());
        }
        let (rect, recorded_sha256) = frame_region_for_evidence_v1(
            &perception.decision_record,
            matches[0].frame_region_evidence_id,
        )?;
        let actual_sha256 =
            visible_frame_region_content_sha256_v1(&source.canonical_bgra8, &size, rect)
                .map_err(|error| format!("rehash opaque gesture target: {error}"))?;
        if actual_sha256 != recorded_sha256 {
            return Err("gesture target evidence differs from retained opaque pixels".to_owned());
        }
        if role == MtgoDuelGestureTargetRoleV1::PrimarySemanticControl {
            if let Some((expected_rect, expected_sha256)) = exact_primary {
                if rect != expected_rect || recorded_sha256 != expected_sha256 {
                    return Err(
                        "source gesture primary differs from the selected visible control"
                            .to_owned(),
                    );
                }
            }
        }
        let center_x = rect
            .x
            .checked_add(rect.width / 2)
            .ok_or("gesture target client x overflow")?;
        let center_y = rect
            .y
            .checked_add(rect.height / 2)
            .ok_or("gesture target client y overflow")?;
        let desktop_x = client_rect
            .left
            .checked_add(i32::try_from(center_x).map_err(|_| "gesture target x is too large")?)
            .ok_or("gesture target desktop x overflow")?;
        let desktop_y = client_rect
            .top
            .checked_add(i32::try_from(center_y).map_err(|_| "gesture target y is too large")?)
            .ok_or("gesture target desktop y overflow")?;
        if !client_rect.contains_point(desktop_x, desktop_y) {
            return Err("gesture target center is outside the current client".to_owned());
        }
        points.push((desktop_x, desktop_y));
        regions.push(PrivateOpaqueGestureTargetRegionV1 {
            role,
            frame_region_evidence_id: matches[0].frame_region_evidence_id,
            rect_client_px: rect.clone(),
            content_sha256: recorded_sha256.to_owned(),
        });
    }
    Ok(ResolvedOpaqueGestureTargetsV1 {
        points_desktop_px: points,
        regions,
    })
}

/// Recaptures and reclassifies the exact current duel state immediately before
/// a future one-click priority Pass actuator. The source and fresh decision
/// payloads, selected visible control, pixels, process incarnation, window,
/// geometry, and output must all still match.
///
/// This function performs no input and returns no coordinate accessor. Other
/// action families remain unsupported until their exact input gestures are
/// separately calibrated.
pub fn prepare_opaque_competitive_duel_pass_actuation_v1(
    plan: OpaqueMtgoCompetitiveDuelActionPlanV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    runtime: &OpaqueMtgoVerifiedDuelPerceptionRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoPreparedCompetitiveDuelPassV1, String> {
    if plan.control.selected_action_family != MtgoDuelActionFamilyV1::PriorityPass {
        return Err(
            "the current competitive actuator preparation supports priority Pass only".to_owned(),
        );
    }
    let plan_commitments = plan.commitments_v1();
    let source_perception = &plan.control.selection.perception;
    let source_manifest = &source_perception.source_frame.source_frame.manifest;
    if source_perception
        .source_frame
        .perception_profile_commitment_sha256
        != profile.perception_profile_commitment_sha256()
        || source_perception
            .source_frame
            .perception_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
    {
        return Err("competitive action plan and duel profile differ before recapture".to_owned());
    }

    let current_frame = capture_admitted_mtgo_duel_visible_frame_v1(profile, timeout_ms)?;
    let current_capture = current_frame.commitments_v1();
    let current_manifest = &current_frame.source_frame.manifest;
    validate_same_duel_window_incarnation_v1(source_manifest, current_manifest)?;
    if current_capture.source_capture.captured_at_unix_millis
        <= source_manifest.captured_at_unix_millis
        || current_capture.source_capture.capture_commitment_sha256
            == source_perception
                .source_frame
                .source_frame
                .capture_commitment_sha256
    {
        return Err("immediate duel capture is not a strictly newer frame".to_owned());
    }

    let immediate_frame_sequence = plan_commitments
        .frame_sequence
        .checked_add(1)
        .ok_or("immediate duel frame sequence overflow")?;
    if immediate_frame_sequence
        > plan_commitments.gameplay_authorization_valid_through_frame_sequence
    {
        return Err("competitive gameplay authorization expired before Pass recapture".to_owned());
    }
    let immediate_frame_id = frame_id_from_capture_commitment_v1(
        &current_capture.source_capture.capture_commitment_sha256,
        plan_commitments.frame_id,
    )?;
    let current_perception = perceive_admitted_duel_frame_v1(
        current_frame,
        profile,
        runtime,
        MtgoDuelPerceptionFrameIdentityV1 {
            frame_id: immediate_frame_id,
            frame_sequence: immediate_frame_sequence,
        },
        timeout_ms,
    )?;
    if current_perception.decision_record.payload != source_perception.decision_record.payload {
        return Err("the visible duel decision payload changed before Pass input".to_owned());
    }

    let mut matching_controls = Vec::new();
    for candidate in &current_perception.visible_controls.controls {
        let semantic_json = serde_json::to_vec(&candidate.semantic)
            .map_err(|error| format!("serialize immediate control semantic: {error}"))?;
        if semantic_json == plan.control.selected_semantic_json {
            matching_controls.push(candidate);
        }
    }
    if matching_controls.len() != 1
        || matching_controls[0].control_id != plan.control.commitments.control_id
    {
        return Err("the immediate Pass control is ambiguous or changed identity".to_owned());
    }
    let selected = matching_controls[0];
    let (current_rect, current_region_sha256) = frame_region_for_evidence_v1(
        &current_perception.decision_record,
        selected.frame_region_evidence_id,
    )?;
    if current_rect != &plan.control.rect_client_px
        || current_region_sha256 != plan.control.selected_region_content_sha256
    {
        return Err("the immediate Pass control geometry or pixels changed".to_owned());
    }

    let current_source = &current_perception.source_frame.source_frame;
    let current_manifest = &current_source.manifest;
    let current_manifest_json = serialize_manifest_v2(current_manifest)
        .map_err(|error| format!("serialize immediate Pass capture manifest: {error}"))?;
    let current_output_bounds = MtgoSignedRectDesktopPxV1 {
        left: current_manifest.output.bounds_desktop_px.left,
        top: current_manifest.output.bounds_desktop_px.top,
        width: current_manifest.output.bounds_desktop_px.width()?,
        height: current_manifest.output.bounds_desktop_px.height()?,
    };
    let current_output_identity_sha256 = preview_output_identity_commitment_v1(
        &current_manifest.output.device_name,
        &current_output_bounds,
    )
    .map_err(|error| format!("immediate Pass output identity is invalid: {error}"))?;
    let before_input_postcondition = check_untrusted_competitive_gameplay_before_input_pixels_v1(
        &plan.competitive,
        MtgoProfileBoundPostconditionBeforeInputFrameV1 {
            schema_version: MTGO_PROFILE_BOUND_POSTCONDITION_BEFORE_INPUT_FRAME_SCHEMA_V1,
            plan_commitment_sha256: plan
                .competitive
                .postcondition_plan_commitment_sha256()
                .to_owned(),
            frame_id: immediate_frame_id,
            frame_sequence: immediate_frame_sequence,
            manifest_sha256: sha256_hex_v1(&current_manifest_json),
            output_identity_sha256: current_output_identity_sha256,
            perception_profile_admission_commitment_sha256: current_perception
                .source_frame
                .perception_profile_admission_commitment_sha256
                .clone(),
            client_size_px: MtgoSizePxV1 {
                width: current_manifest.frame.canonical_width,
                height: current_manifest.frame.canonical_height,
            },
            capture_role: MtgoDxgiCaptureRoleV2::ActingPlayerDuel,
        },
        &current_source.canonical_bgra8,
    )
    .map_err(|error| format!("verify immediate Pass postcondition baseline: {error}"))?;

    let client_rect = current_perception
        .source_frame
        .source_frame
        .manifest
        .pre
        .client_rect_desktop_px;
    let target_x_client_px = current_rect
        .x
        .checked_add(current_rect.width / 2)
        .ok_or("immediate Pass target x overflow")?;
    let target_y_client_px = current_rect
        .y
        .checked_add(current_rect.height / 2)
        .ok_or("immediate Pass target y overflow")?;
    let target_x_desktop_px = client_rect
        .left
        .checked_add(
            i32::try_from(target_x_client_px)
                .map_err(|_| "immediate Pass target x does not fit the desktop")?,
        )
        .ok_or("immediate Pass desktop x overflow")?;
    let target_y_desktop_px = client_rect
        .top
        .checked_add(
            i32::try_from(target_y_client_px)
                .map_err(|_| "immediate Pass target y does not fit the desktop")?,
        )
        .ok_or("immediate Pass desktop y overflow")?;
    if !client_rect.contains_point(target_x_desktop_px, target_y_desktop_px) {
        return Err("immediate Pass target is outside the current client".to_owned());
    }
    let output_bounds = current_perception
        .source_frame
        .source_frame
        .manifest
        .output
        .bounds_desktop_px;
    let (park_x_desktop_px, park_y_desktop_px) =
        choose_cursor_park_point_v3(&client_rect, &output_bounds)?;
    let current_perception_commitments = current_perception.commitments_v1();
    let event_kind_json = serde_json::to_vec(&plan_commitments.event_kind)
        .map_err(|error| format!("serialize competitive event kind: {error}"))?;
    let preparation_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_COMPETITIVE_PASS_PREPARATION_DOMAIN_V1,
        &[
            plan_commitments
                .opaque_competitive_action_plan_commitment_sha256
                .as_bytes(),
            plan_commitments
                .competitive_mode_authorization_commitment_sha256
                .as_bytes(),
            plan_commitments
                .competitive_authorization_commitment_sha256
                .as_bytes(),
            before_input_postcondition
                .verification_commitment_sha256()
                .as_bytes(),
            current_capture
                .source_capture
                .capture_commitment_sha256
                .as_bytes(),
            current_perception_commitments
                .perception_result_commitment_sha256
                .as_bytes(),
            plan.control.commitments.control_id.as_bytes(),
            &plan.control.selected_semantic_json,
            &serde_json::to_vec(current_rect)
                .map_err(|error| format!("serialize immediate Pass rectangle: {error}"))?,
            current_region_sha256.as_bytes(),
            &event_kind_json,
            &[plan_commitments.game_number],
            b"prepared_no_input_or_event_entry_authority",
        ],
    );
    let commitments = MtgoOpaqueCompetitiveDuelPassPreparationCommitmentsV1 {
        competitive_action_plan_commitment_sha256: plan_commitments
            .opaque_competitive_action_plan_commitment_sha256,
        competitive_mode_authorization_commitment_sha256: plan_commitments
            .competitive_mode_authorization_commitment_sha256,
        competitive_match_gameplay_authorization_commitment_sha256: plan_commitments
            .competitive_authorization_commitment_sha256,
        before_input_postcondition_verification_commitment_sha256: before_input_postcondition
            .verification_commitment_sha256()
            .to_owned(),
        immediate_capture_commitment_sha256: current_capture
            .source_capture
            .capture_commitment_sha256,
        immediate_perception_result_commitment_sha256: current_perception_commitments
            .perception_result_commitment_sha256,
        preparation_commitment_sha256,
        event_kind: plan_commitments.event_kind,
        game_number: plan_commitments.game_number,
        immediate_frame_id,
        immediate_frame_sequence,
        immediate_captured_at_unix_millis: current_capture.source_capture.captured_at_unix_millis,
    };
    let (current_hwnd, current_process_id, current_process_start_filetime_100ns, current_dpi) = {
        let current_snapshot = &current_perception.source_frame.source_frame.manifest.pre;
        (
            current_snapshot.hwnd,
            current_snapshot.process_id,
            current_snapshot.process_start_filetime_100ns,
            current_snapshot.dpi,
        )
    };
    Ok(OpaqueMtgoPreparedCompetitiveDuelPassV1 {
        _plan: plan,
        _current_perception: current_perception,
        _before_input_postcondition: before_input_postcondition,
        commitments,
        hwnd: current_hwnd,
        process_id: current_process_id,
        process_start_filetime_100ns: current_process_start_filetime_100ns,
        dpi: current_dpi,
        client_rect_desktop_px: client_rect,
        target_x_desktop_px,
        target_y_desktop_px,
        park_x_desktop_px,
        park_y_desktop_px,
    })
}

pub(crate) fn confirm_opaque_competitive_duel_pass_postcondition_v1(
    prepared: OpaqueMtgoPreparedCompetitiveDuelPassV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoConfirmedCompetitiveDuelPassV1, String> {
    if !(100..=10_000).contains(&timeout_ms) {
        return Err(
            "competitive Pass confirmation timeout must be between 100 and 10000 ms".to_owned(),
        );
    }
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(u64::from(timeout_ms)))
        .ok_or("competitive Pass confirmation deadline overflow")?;
    let prepared_commitments = prepared.commitments_v1();
    let OpaqueMtgoPreparedCompetitiveDuelPassV1 {
        _plan: action_plan,
        _current_perception: current_perception,
        _before_input_postcondition: before_input_postcondition,
        ..
    } = prepared;
    if current_perception
        .source_frame
        .perception_profile_commitment_sha256
        != profile.perception_profile_commitment_sha256()
        || current_perception
            .source_frame
            .perception_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
        || before_input_postcondition.frame_id() != prepared_commitments.immediate_frame_id
        || before_input_postcondition.frame_sequence()
            != prepared_commitments.immediate_frame_sequence
        || before_input_postcondition.canonical_bgra8_sha256()
            != current_perception
                .source_frame
                .source_frame
                .manifest
                .frame
                .canonical_bgra8_sha256
        || before_input_postcondition.verification_commitment_sha256()
            != prepared_commitments.before_input_postcondition_verification_commitment_sha256
    {
        return Err(
            "prepared Pass baseline no longer matches the admitted duel profile".to_owned(),
        );
    }

    let current_manifest = &current_perception.source_frame.source_frame.manifest;
    let postcondition_plan_commitment_sha256 = action_plan
        .competitive
        .postcondition_plan_commitment_sha256()
        .to_owned();
    let mut postcondition_candidate_count = 0_u32;
    let (after_frame, after_capture, after_frame_id, after_frame_sequence, checked_postcondition) = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining < Duration::from_millis(100) {
            return Err(format!(
                    "timed out waiting for the complete competitive Pass visible postcondition after {postcondition_candidate_count} admitted candidates"
                ));
        }
        let candidate_timeout_ms = u32::try_from(remaining.as_millis())
            .unwrap_or(u32::MAX)
            .clamp(100, 10_000);
        let after_frame =
            capture_admitted_mtgo_duel_visible_frame_v1(profile, candidate_timeout_ms)?;
        postcondition_candidate_count = postcondition_candidate_count
            .checked_add(1)
            .ok_or("competitive Pass postcondition candidate count overflow")?;
        let after_capture = after_frame.commitments_v1();
        let after_manifest = &after_frame.source_frame.manifest;
        validate_same_duel_window_incarnation_v1(current_manifest, after_manifest)?;
        if after_capture.source_capture.captured_at_unix_millis
            < prepared_commitments.immediate_captured_at_unix_millis
        {
            return Err("competitive Pass postcondition clock moved backwards".to_owned());
        }
        if after_capture.source_capture.captured_at_unix_millis
            == prepared_commitments.immediate_captured_at_unix_millis
            || after_capture.source_capture.capture_commitment_sha256
                == prepared_commitments.immediate_capture_commitment_sha256
        {
            thread::sleep(Duration::from_millis(25));
            continue;
        }
        let after_frame_sequence = prepared_commitments
            .immediate_frame_sequence
            .checked_add(u64::from(postcondition_candidate_count))
            .ok_or("competitive Pass postcondition frame sequence overflow")?;
        let after_frame_id = frame_id_from_capture_commitment_v1(
            &after_capture.source_capture.capture_commitment_sha256,
            prepared_commitments.immediate_frame_id,
        )?;
        let after_manifest_json = serialize_manifest_v2(after_manifest)
            .map_err(|error| format!("serialize competitive Pass after manifest: {error}"))?;
        let after_output_bounds = MtgoSignedRectDesktopPxV1 {
            left: after_manifest.output.bounds_desktop_px.left,
            top: after_manifest.output.bounds_desktop_px.top,
            width: after_manifest.output.bounds_desktop_px.width()?,
            height: after_manifest.output.bounds_desktop_px.height()?,
        };
        let after_output_identity_sha256 = preview_output_identity_commitment_v1(
            &after_manifest.output.device_name,
            &after_output_bounds,
        )
        .map_err(|error| format!("competitive Pass after output identity is invalid: {error}"))?;
        let after_metadata = MtgoProfileBoundPostconditionAfterFrameMetadataV1 {
            schema_version: MTGO_PROFILE_BOUND_POSTCONDITION_AFTER_FRAME_SCHEMA_V1,
            plan_commitment_sha256: postcondition_plan_commitment_sha256.clone(),
            frame_id: after_frame_id,
            frame_sequence: after_frame_sequence,
            manifest_sha256: sha256_hex_v1(&after_manifest_json),
            output_identity_sha256: after_output_identity_sha256,
            perception_profile_admission_commitment_sha256: after_frame
                .perception_profile_admission_commitment_sha256
                .clone(),
            client_size_px: MtgoSizePxV1 {
                width: after_manifest.frame.canonical_width,
                height: after_manifest.frame.canonical_height,
            },
            capture_role: MtgoDxgiCaptureRoleV2::ActingPlayerDuel,
        };
        match inspect_untrusted_competitive_gameplay_postcondition_candidate_pixels_v1(
            &action_plan.competitive,
            after_metadata.clone(),
            &after_frame.source_frame.canonical_bgra8,
        )
        .map_err(|error| format!("inspect competitive Pass visible postcondition: {error}"))?
        {
            MtgoProfileBoundPostconditionCandidateStatusV1::PendingVisibleChange => {
                thread::sleep(Duration::from_millis(25));
            }
            MtgoProfileBoundPostconditionCandidateStatusV1::CompleteVisibleChange => {
                let checked = check_untrusted_competitive_gameplay_postcondition_pixels_v1(
                    action_plan.competitive,
                    after_metadata,
                    &after_frame.source_frame.canonical_bgra8,
                )
                .map_err(|error| {
                    format!("confirm competitive Pass visible postcondition: {error}")
                })?;
                break (
                    after_frame,
                    after_capture,
                    after_frame_id,
                    after_frame_sequence,
                    checked,
                );
            }
        }
    };
    if checked_postcondition.event_kind() != prepared_commitments.event_kind
        || checked_postcondition.game_number() != prepared_commitments.game_number
        || checked_postcondition.after_frame_id() != after_frame_id
        || checked_postcondition.after_frame_sequence() != after_frame_sequence
    {
        return Err("competitive Pass confirmation changed its exact event or frame".to_owned());
    }
    let event_kind_json = serde_json::to_vec(&prepared_commitments.event_kind)
        .map_err(|error| format!("serialize confirmed competitive event kind: {error}"))?;
    let checked_postcondition_commitment_sha256 = checked_postcondition
        .confirmation_commitment_sha256()
        .to_owned();
    let opaque_confirmation_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_COMPETITIVE_PASS_CONFIRMATION_DOMAIN_V1,
        &[
            prepared_commitments
                .preparation_commitment_sha256
                .as_bytes(),
            prepared_commitments
                .before_input_postcondition_verification_commitment_sha256
                .as_bytes(),
            after_capture
                .source_capture
                .capture_commitment_sha256
                .as_bytes(),
            checked_postcondition
                .confirmation_commitment_sha256()
                .as_bytes(),
            event_kind_json.as_slice(),
            &[prepared_commitments.game_number],
            &after_frame_id.to_be_bytes(),
            &after_frame_sequence.to_be_bytes(),
            &postcondition_candidate_count.to_be_bytes(),
            b"opaque_visible_postcondition_confirmed_no_input_or_event_entry_authority",
        ],
    );
    Ok(OpaqueMtgoConfirmedCompetitiveDuelPassV1 {
        _before_input_postcondition: before_input_postcondition,
        checked_postcondition,
        _after_frame: after_frame,
        commitments: MtgoOpaqueCompetitiveDuelPassConfirmationCommitmentsV1 {
            before_input_verification_commitment_sha256: prepared_commitments
                .before_input_postcondition_verification_commitment_sha256,
            after_capture_commitment_sha256: after_capture.source_capture.capture_commitment_sha256,
            checked_postcondition_commitment_sha256,
            opaque_confirmation_commitment_sha256,
            event_kind: prepared_commitments.event_kind,
            game_number: prepared_commitments.game_number,
            after_frame_id,
            after_frame_sequence,
            postcondition_candidate_count,
        },
    })
}

pub(crate) fn confirm_opaque_competitive_duel_gesture_postcondition_v1(
    prepared: OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    input_sent_at_unix_millis: u128,
    timeout_ms: u32,
) -> Result<OpaqueMtgoConfirmedCompetitiveDuelGestureV1, String> {
    if !(100..=10_000).contains(&timeout_ms) {
        return Err(
            "competitive gesture confirmation timeout must be between 100 and 10000 ms".to_owned(),
        );
    }
    let prepared_commitments = prepared.commitments.clone();
    if prepared_commitments.stage_index.checked_add(1)
        != Some(prepared_commitments.gesture_stage_count)
    {
        return Err("complete-action confirmation requires the final gesture stage".to_owned());
    }
    if input_sent_at_unix_millis == 0
        || input_sent_at_unix_millis < prepared_commitments.fresh_captured_at_unix_millis
    {
        return Err(
            "competitive gesture input time precedes its fresh visible baseline".to_owned(),
        );
    }
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(u64::from(timeout_ms)))
        .ok_or("competitive gesture confirmation deadline overflow")?;
    let OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1 {
        _sequence: sequence,
        _fresh_perception: current_perception,
        _before_input_postcondition: before_input_postcondition,
        ..
    } = prepared;
    let action_plan = sequence.current_stage._plan;
    if current_perception
        .source_frame
        .perception_profile_commitment_sha256
        != profile.perception_profile_commitment_sha256()
        || current_perception
            .source_frame
            .perception_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
        || before_input_postcondition.frame_id() != prepared_commitments.fresh_frame_id
        || before_input_postcondition.frame_sequence() != prepared_commitments.fresh_frame_sequence
        || before_input_postcondition.canonical_bgra8_sha256()
            != current_perception
                .source_frame
                .source_frame
                .manifest
                .frame
                .canonical_bgra8_sha256
        || before_input_postcondition.verification_commitment_sha256()
            != prepared_commitments.before_input_postcondition_verification_commitment_sha256
    {
        return Err(
            "prepared gesture baseline no longer matches the admitted duel profile".to_owned(),
        );
    }

    let current_manifest = &current_perception.source_frame.source_frame.manifest;
    let postcondition_plan_commitment_sha256 = action_plan
        .competitive
        .postcondition_plan_commitment_sha256()
        .to_owned();
    let mut postcondition_candidate_count = 0_u32;
    let (after_frame, after_capture, after_frame_id, after_frame_sequence, checked_postcondition) = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining < Duration::from_millis(100) {
            return Err(format!(
                    "timed out waiting for the complete competitive gesture visible postcondition after {postcondition_candidate_count} admitted candidates"
                ));
        }
        let candidate_timeout_ms = u32::try_from(remaining.as_millis())
            .unwrap_or(u32::MAX)
            .clamp(100, 10_000);
        let after_frame =
            capture_admitted_mtgo_duel_visible_frame_v1(profile, candidate_timeout_ms)?;
        postcondition_candidate_count = postcondition_candidate_count
            .checked_add(1)
            .ok_or("competitive gesture postcondition candidate count overflow")?;
        let after_capture = after_frame.commitments_v1();
        let after_manifest = &after_frame.source_frame.manifest;
        validate_same_duel_window_incarnation_v1(current_manifest, after_manifest)?;
        if after_capture.source_capture.captured_at_unix_millis < input_sent_at_unix_millis {
            return Err(
                "competitive gesture postcondition capture predates its input receipt".to_owned(),
            );
        }
        if after_capture.source_capture.captured_at_unix_millis
            == prepared_commitments.fresh_captured_at_unix_millis
            || after_capture.source_capture.capture_commitment_sha256
                == prepared_commitments.fresh_capture_commitment_sha256
        {
            thread::sleep(Duration::from_millis(25));
            continue;
        }
        let after_frame_sequence = prepared_commitments
            .fresh_frame_sequence
            .checked_add(u64::from(postcondition_candidate_count))
            .ok_or("competitive gesture postcondition frame sequence overflow")?;
        if after_frame_sequence
            > prepared_commitments.gameplay_authorization_valid_through_frame_sequence
        {
            return Err(
                "competitive gesture postcondition exceeded the game authorization lifetime"
                    .to_owned(),
            );
        }
        let after_frame_id = frame_id_from_capture_commitment_v1(
            &after_capture.source_capture.capture_commitment_sha256,
            prepared_commitments.fresh_frame_id,
        )?;
        let after_manifest_json = serialize_manifest_v2(after_manifest)
            .map_err(|error| format!("serialize competitive gesture after manifest: {error}"))?;
        let after_output_bounds = MtgoSignedRectDesktopPxV1 {
            left: after_manifest.output.bounds_desktop_px.left,
            top: after_manifest.output.bounds_desktop_px.top,
            width: after_manifest.output.bounds_desktop_px.width()?,
            height: after_manifest.output.bounds_desktop_px.height()?,
        };
        let after_output_identity_sha256 = preview_output_identity_commitment_v1(
            &after_manifest.output.device_name,
            &after_output_bounds,
        )
        .map_err(|error| {
            format!("competitive gesture after output identity is invalid: {error}")
        })?;
        let after_metadata = MtgoProfileBoundPostconditionAfterFrameMetadataV1 {
            schema_version: MTGO_PROFILE_BOUND_POSTCONDITION_AFTER_FRAME_SCHEMA_V1,
            plan_commitment_sha256: postcondition_plan_commitment_sha256.clone(),
            frame_id: after_frame_id,
            frame_sequence: after_frame_sequence,
            manifest_sha256: sha256_hex_v1(&after_manifest_json),
            output_identity_sha256: after_output_identity_sha256,
            perception_profile_admission_commitment_sha256: after_frame
                .perception_profile_admission_commitment_sha256
                .clone(),
            client_size_px: MtgoSizePxV1 {
                width: after_manifest.frame.canonical_width,
                height: after_manifest.frame.canonical_height,
            },
            capture_role: MtgoDxgiCaptureRoleV2::ActingPlayerDuel,
        };
        match inspect_untrusted_competitive_gameplay_postcondition_candidate_pixels_v1(
            &action_plan.competitive,
            after_metadata.clone(),
            &after_frame.source_frame.canonical_bgra8,
        )
        .map_err(|error| format!("inspect competitive gesture visible postcondition: {error}"))?
        {
            MtgoProfileBoundPostconditionCandidateStatusV1::PendingVisibleChange => {
                thread::sleep(Duration::from_millis(25));
            }
            MtgoProfileBoundPostconditionCandidateStatusV1::CompleteVisibleChange => {
                let checked = check_untrusted_competitive_gameplay_postcondition_pixels_v1(
                    action_plan.competitive,
                    after_metadata,
                    &after_frame.source_frame.canonical_bgra8,
                )
                .map_err(|error| {
                    format!("confirm competitive gesture visible postcondition: {error}")
                })?;
                break (
                    after_frame,
                    after_capture,
                    after_frame_id,
                    after_frame_sequence,
                    checked,
                );
            }
        }
    };
    if checked_postcondition.event_kind() != prepared_commitments.event_kind
        || checked_postcondition.game_number() != prepared_commitments.game_number
        || checked_postcondition.after_frame_id() != after_frame_id
        || checked_postcondition.after_frame_sequence() != after_frame_sequence
    {
        return Err("competitive gesture confirmation changed its exact event or frame".to_owned());
    }
    let family_json = serde_json::to_vec(&prepared_commitments.selected_action_family)
        .map_err(|error| format!("serialize confirmed gesture family: {error}"))?;
    let event_kind_json = serde_json::to_vec(&prepared_commitments.event_kind)
        .map_err(|error| format!("serialize confirmed gesture event kind: {error}"))?;
    let checked_postcondition_commitment_sha256 = checked_postcondition
        .confirmation_commitment_sha256()
        .to_owned();
    let opaque_confirmation_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_COMPETITIVE_GESTURE_CONFIRMATION_DOMAIN_V1,
        &[
            prepared_commitments
                .preparation_commitment_sha256
                .as_bytes(),
            prepared_commitments
                .gesture_target_runtime_identity_commitment_sha256
                .as_bytes(),
            prepared_commitments
                .gesture_target_request_commitment_sha256
                .as_bytes(),
            prepared_commitments
                .before_input_postcondition_verification_commitment_sha256
                .as_bytes(),
            after_capture
                .source_capture
                .capture_commitment_sha256
                .as_bytes(),
            checked_postcondition
                .confirmation_commitment_sha256()
                .as_bytes(),
            &family_json,
            &event_kind_json,
            &[prepared_commitments.game_number],
            &after_frame_id.to_be_bytes(),
            &after_frame_sequence.to_be_bytes(),
            &postcondition_candidate_count.to_be_bytes(),
            b"one_stage_gesture_visible_postcondition_confirmed_no_input_authority",
        ],
    );
    Ok(OpaqueMtgoConfirmedCompetitiveDuelGestureV1 {
        _before_input_postcondition: before_input_postcondition,
        checked_postcondition,
        _after_frame: after_frame,
        commitments: MtgoOpaqueCompetitiveDuelGestureConfirmationCommitmentsV1 {
            before_input_verification_commitment_sha256: prepared_commitments
                .before_input_postcondition_verification_commitment_sha256,
            after_capture_commitment_sha256: after_capture.source_capture.capture_commitment_sha256,
            checked_postcondition_commitment_sha256,
            opaque_confirmation_commitment_sha256,
            selected_action_family: prepared_commitments.selected_action_family,
            event_kind: prepared_commitments.event_kind,
            game_number: prepared_commitments.game_number,
            after_frame_id,
            after_frame_sequence,
            postcondition_candidate_count,
        },
    })
}

/// Joins the opaque Windows capture-to-control chain to the separately checked
/// competitive postcondition and authorization plan. Every shared identity is
/// compared before either move-only input is retained.
pub(crate) fn bind_opaque_duel_control_to_competitive_action_plan_v1(
    control: OpaqueMtgoProfileBoundDuelResolvedControlV1,
    gesture: CheckedUntrustedMtgoDuelGesturePlanV1,
    competitive: CheckedUntrustedMtgoCompetitiveGameplayActionPlanV1,
) -> Result<OpaqueMtgoCompetitiveDuelActionPlanV1, String> {
    let opaque = opaque_duel_action_binding_view_v1(&control)?;
    let gesture_commitments = gesture.commitments_v1();
    let postcondition = competitive.postcondition_plan_commitments_v1();
    validate_opaque_competitive_action_binding_v1(&opaque, &postcondition)?;
    if gesture_commitments.profile_bound_resolution_commitment_sha256
        != opaque.profile_bound_resolution_commitment_sha256
        || gesture_commitments.decision_commitment_sha256 != opaque.decision_commitment_sha256
        || gesture_commitments.selection_commitment_sha256 != opaque.selection_commitment_sha256
        || gesture_commitments.frame_id != opaque.frame_id
        || gesture_commitments.frame_sequence != opaque.frame_sequence
        || gesture_commitments.selected_action_family != control.selected_action_family
    {
        return Err("gesture plan does not retain the exact opaque duel control".to_owned());
    }

    let selected_semantic_json = opaque.selected_semantic_json.clone();
    let opaque_competitive_action_plan_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_COMPETITIVE_ACTION_PLAN_DOMAIN_V1,
        &[
            opaque
                .opaque_control_resolution_commitment_sha256
                .as_bytes(),
            gesture_commitments.plan_commitment_sha256.as_bytes(),
            &gesture_commitments.stage_count.to_be_bytes(),
            postcondition.plan_commitment_sha256.as_bytes(),
            competitive.competitive_scope_commitment_sha256().as_bytes(),
            competitive
                .mode_authorization_commitment_sha256()
                .as_bytes(),
            competitive.authorization_commitment_sha256().as_bytes(),
            &selected_semantic_json,
            b"opaque_coordinates_and_postconditions_retained_no_input_or_event_entry_authority",
        ],
    );
    Ok(OpaqueMtgoCompetitiveDuelActionPlanV1 {
        control,
        gesture,
        competitive,
        opaque_competitive_action_plan_commitment_sha256,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpaqueDuelActionBindingViewV1 {
    opaque_control_resolution_commitment_sha256: String,
    profile_bound_resolution_commitment_sha256: String,
    decision_commitment_sha256: String,
    selection_commitment_sha256: String,
    control_resolution_commitment_sha256: String,
    perception_profile_admission_commitment_sha256: String,
    deployment_commitment_sha256: String,
    control_id: String,
    frame_id: u64,
    frame_sequence: u64,
    source_manifest_sha256: String,
    source_frame_sha256: String,
    source_output_identity_sha256: String,
    source_client_size_px: MtgoSizePxV1,
    selected_semantic_json: Vec<u8>,
}

fn opaque_duel_action_binding_view_v1(
    control: &OpaqueMtgoProfileBoundDuelResolvedControlV1,
) -> Result<OpaqueDuelActionBindingViewV1, String> {
    let commitments = control.commitments_v1();
    let perception = &control.selection.perception;
    let source_frame = &perception.source_frame;
    let source = &source_frame.source_frame;
    let manifest = &source.manifest;
    let width = manifest.frame.canonical_width;
    let height = manifest.frame.canonical_height;
    let expected_length = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("opaque duel action source byte length overflow")?;
    if manifest.frame.canonical_stride != width.checked_mul(4).ok_or("stride overflow")?
        || manifest.frame.canonical_byte_length != expected_length
        || source.canonical_bgra8.len() != expected_length
        || manifest.frame.canonical_bgra8_sha256 != sha256_hex_v1(&source.canonical_bgra8)
    {
        return Err("opaque duel action source pixels no longer match the capture".to_owned());
    }
    let manifest_json = serialize_manifest_v2(manifest)
        .map_err(|error| format!("serialize opaque duel source manifest: {error}"))?;
    let output_bounds = MtgoSignedRectDesktopPxV1 {
        left: manifest.output.bounds_desktop_px.left,
        top: manifest.output.bounds_desktop_px.top,
        width: manifest.output.bounds_desktop_px.width()?,
        height: manifest.output.bounds_desktop_px.height()?,
    };
    let output_identity_sha256 =
        preview_output_identity_commitment_v1(&manifest.output.device_name, &output_bounds)
            .map_err(|error| format!("opaque duel output identity is invalid: {error}"))?;
    Ok(OpaqueDuelActionBindingViewV1 {
        opaque_control_resolution_commitment_sha256: commitments
            .opaque_control_resolution_commitment_sha256,
        profile_bound_resolution_commitment_sha256: commitments
            .profile_bound_resolution_commitment_sha256,
        decision_commitment_sha256: commitments.decision_commitment_sha256,
        selection_commitment_sha256: commitments.selection_commitment_sha256,
        control_resolution_commitment_sha256: commitments.control_resolution_commitment_sha256,
        perception_profile_admission_commitment_sha256: source_frame
            .perception_profile_admission_commitment_sha256
            .clone(),
        deployment_commitment_sha256: control.selection.deployment_commitment_sha256.clone(),
        control_id: commitments.control_id,
        frame_id: commitments.frame_id,
        frame_sequence: commitments.frame_sequence,
        source_manifest_sha256: sha256_hex_v1(&manifest_json),
        source_frame_sha256: manifest.frame.canonical_bgra8_sha256.clone(),
        source_output_identity_sha256: output_identity_sha256,
        source_client_size_px: MtgoSizePxV1 { width, height },
        selected_semantic_json: control.selected_semantic_json.clone(),
    })
}

fn validate_same_duel_window_incarnation_v1(
    source: &super::CaptureManifestV2,
    current: &super::CaptureManifestV2,
) -> Result<(), String> {
    if source.window_mode != "duel_game"
        || source.capture_role != "acting_player_duel"
        || current.window_mode != source.window_mode
        || current.capture_role != source.capture_role
        || current.expected_game_format != source.expected_game_format
        || current.pre.hwnd != source.pre.hwnd
        || current.post.hwnd != source.post.hwnd
        || current.pre.process_id != source.pre.process_id
        || current.post.process_id != source.post.process_id
        || current.pre.process_start_filetime_100ns != source.pre.process_start_filetime_100ns
        || current.post.process_start_filetime_100ns != source.post.process_start_filetime_100ns
        || current.pre.process_image != source.pre.process_image
        || current.post.process_image != source.post.process_image
        || current.pre.executable_sha256 != source.pre.executable_sha256
        || current.post.executable_sha256 != source.post.executable_sha256
        || current.pre.signer_thumbprint != source.pre.signer_thumbprint
        || current.post.signer_thumbprint != source.post.signer_thumbprint
        || current.pre.signer_subject_sha256 != source.pre.signer_subject_sha256
        || current.post.signer_subject_sha256 != source.post.signer_subject_sha256
        || current.pre.title != source.pre.title
        || current.post.title != source.post.title
        || current.pre.dpi != source.pre.dpi
        || current.post.dpi != source.post.dpi
        || current.pre.client_rect_desktop_px != source.pre.client_rect_desktop_px
        || current.post.client_rect_desktop_px != source.post.client_rect_desktop_px
        || current.pre.extended_frame_rect_desktop_px != source.pre.extended_frame_rect_desktop_px
        || current.post.extended_frame_rect_desktop_px != source.post.extended_frame_rect_desktop_px
        || current.output != source.output
        || current.frame.source_texture_width != source.frame.source_texture_width
        || current.frame.source_texture_height != source.frame.source_texture_height
        || current.frame.source_texture_format != source.frame.source_texture_format
        || current.frame.canonical_width != source.frame.canonical_width
        || current.frame.canonical_height != source.frame.canonical_height
        || current.frame.canonical_stride != source.frame.canonical_stride
    {
        return Err(
            "the immediate capture changed the duel window or process incarnation".to_owned(),
        );
    }
    Ok(())
}

pub(super) fn frame_id_from_capture_commitment_v1(
    capture_commitment_sha256: &str,
    source_frame_id: u64,
) -> Result<u64, String> {
    if !looks_like_lower_sha256_v1(capture_commitment_sha256) {
        return Err("immediate capture commitment is not lowercase SHA-256".to_owned());
    }
    let mut frame_id = u64::from_str_radix(&capture_commitment_sha256[..16], 16)
        .map_err(|error| format!("derive immediate frame id: {error}"))?;
    if frame_id == 0 || frame_id == source_frame_id {
        frame_id ^= 0xa5a5_5a5a_d3d3_3c3c;
    }
    if frame_id == 0 || frame_id == source_frame_id {
        return Err("immediate capture cannot derive a distinct nonzero frame id".to_owned());
    }
    Ok(frame_id)
}

fn validate_opaque_competitive_action_binding_v1(
    opaque: &OpaqueDuelActionBindingViewV1,
    postcondition: &mtgo_blackbox_v1::MtgoProfileBoundActionPostconditionPlanCommitmentsV1,
) -> Result<(), String> {
    if opaque.decision_commitment_sha256 != postcondition.decision_commitment_sha256
        || opaque.selection_commitment_sha256 != postcondition.selection_commitment_sha256
        || opaque.control_resolution_commitment_sha256
            != postcondition.control_resolution_commitment_sha256
        || opaque.perception_profile_admission_commitment_sha256
            != postcondition.perception_profile_admission_commitment_sha256
        || opaque.deployment_commitment_sha256 != postcondition.deployment_commitment_sha256
        || opaque.control_id != postcondition.control_id
        || opaque.frame_id != postcondition.frame_id
        || opaque.frame_sequence != postcondition.frame_sequence
        || opaque.source_manifest_sha256 != postcondition.source_manifest_sha256
        || opaque.source_frame_sha256 != postcondition.source_frame_sha256
        || opaque.source_output_identity_sha256 != postcondition.source_output_identity_sha256
        || opaque.source_client_size_px != postcondition.source_client_size_px
        || selected_semantic_sha256_v1(&opaque.selected_semantic_json)
            != postcondition.selected_semantic_sha256
    {
        return Err(
            "competitive duel plan does not bind the exact opaque source, model selection, and visible control"
                .to_owned(),
        );
    }
    Ok(())
}

fn selected_semantic_sha256_v1(semantic_json: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"mtgo-selected-duel-semantic-v1");
    hasher.update((semantic_json.len() as u64).to_be_bytes());
    hasher.update(semantic_json);
    format!("{:x}", hasher.finalize())
}

fn validate_and_bind_perception_record_v1(
    record: &MtgoObservedDecisionV1,
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
    source_frame_sha256: &str,
    identity: MtgoDuelPerceptionFrameIdentityV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
) -> Result<ValidatedMtgoObservedDecisionV1, String> {
    if record.frames.len() != 1 {
        return Err("live duel perception must describe exactly one current frame".to_owned());
    }
    let frame = &record.frames[0];
    if record.frame_id != identity.frame_id
        || frame.frame_id != identity.frame_id
        || frame.sequence != identity.frame_sequence
        || frame.sha256 != source_frame_sha256
        || frame.client_bounds.x != 0
        || frame.client_bounds.y != 0
        || frame.client_bounds.width != size.width
        || frame.client_bounds.height != size.height
    {
        return Err("perception decision frame does not match the opaque source".to_owned());
    }
    let validated = validate_observed_decision_v1(record.clone())
        .map_err(|error| format!("perception decision contract failed: {error}"))?;
    for evidence in &record.evidence {
        match &evidence.source {
            MtgoEvidenceSourceV1::FrameRegion {
                frame_id,
                rect,
                content_sha256,
            } => {
                if *frame_id != identity.frame_id {
                    return Err("visible evidence cites a different frame".to_owned());
                }
                let actual = visible_frame_region_content_sha256_v1(canonical_bgra8, size, rect)
                    .map_err(|error| format!("visible evidence region is invalid: {error}"))?;
                if actual != *content_sha256 {
                    return Err(
                        "visible evidence region hash differs from opaque pixels".to_owned()
                    );
                }
            }
            MtgoEvidenceSourceV1::VisibleAccessibilityText { .. } => {
                return Err(
                    "live duel perception cannot use accessibility text as game evidence"
                        .to_owned(),
                );
            }
            MtgoEvidenceSourceV1::ManualVisibleAnnotation { .. } => {
                return Err(
                    "live duel perception cannot use a manual annotation as runtime evidence"
                        .to_owned(),
                );
            }
            MtgoEvidenceSourceV1::VisibleGameLogText { .. }
            | MtgoEvidenceSourceV1::DerivedPublicFact { .. } => {}
        }
    }
    for action in validated.legal_actions() {
        let family = duel_action_family_v1(action);
        if !profile.supported_action_families().contains(&family) {
            return Err(format!(
                "perception emitted an action family outside the admitted profile: {family:?}"
            ));
        }
    }
    Ok(validated)
}

fn validate_reconstruction_audit_pixels_v1(
    audit: &MtgoObservationReconstructionAuditV1,
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
) -> Result<(), String> {
    for group in &audit.groups {
        for region in &group.visible_regions {
            let actual = visible_frame_region_content_sha256_v1(
                canonical_bgra8,
                size,
                &region.rect_client_px,
            )
            .map_err(|error| format!("reconstruction audit region is invalid: {error}"))?;
            if actual != region.bgra8_sha256 {
                return Err(
                    "reconstruction audit region hash differs from opaque pixels".to_owned(),
                );
            }
        }
    }
    Ok(())
}

fn validate_visible_control_set_candidate_v1(
    record: &MtgoObservedDecisionV1,
    validated: &ValidatedMtgoObservedDecisionV1,
    control_set: &MtgoVisibleActionControlSetV1,
) -> Result<(), String> {
    if control_set.schema_version != MTGO_VISIBLE_ACTION_CONTROL_SET_SCHEMA_V1
        || control_set.decision_commitment_sha256 != validated.decision_commitment_sha256()
        || control_set.frame_id != validated.frame_id()
        || control_set.frame_sequence != validated.frame_sequence()
        || !control_set.prompt_reconciled
        || !control_set.candidate_set_complete
    {
        return Err(
            "perception visible-control set does not bind the complete decision".to_owned(),
        );
    }
    if control_set.controls.is_empty()
        || control_set.controls.len() > 128
        || control_set.controls.len() != validated.legal_actions().len()
    {
        return Err(
            "perception visible-control set must map every legal action exactly once".to_owned(),
        );
    }
    let (prompt_rect, _) =
        frame_region_for_evidence_v1(record, control_set.prompt_frame_region_evidence_id)?;
    let mut control_ids = std::collections::HashSet::new();
    let mut evidence_ids = std::collections::HashSet::new();
    for control in &control_set.controls {
        if control.control_id.is_empty()
            || control.control_id.len() > 128
            || !control.control_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
            })
            || !control_ids.insert(control.control_id.as_str())
            || !evidence_ids.insert(control.frame_region_evidence_id)
            || !control.visibly_enabled
            || !(MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1..=10_000).contains(&control.confidence_bps)
        {
            return Err("perception visible-control candidate is invalid or duplicated".to_owned());
        }
        if validated
            .legal_actions()
            .iter()
            .filter(|action| *action == &control.semantic)
            .count()
            != 1
            || control_set
                .controls
                .iter()
                .filter(|candidate| candidate.semantic == control.semantic)
                .count()
                != 1
        {
            return Err(
                "perception visible-control semantics must map one-to-one to legal actions"
                    .to_owned(),
            );
        }
        let (control_rect, _) =
            frame_region_for_evidence_v1(record, control.frame_region_evidence_id)?;
        if rects_intersect_v1(prompt_rect, control_rect) {
            return Err("perception visible control overlaps the prompt evidence".to_owned());
        }
    }
    Ok(())
}

fn frame_region_for_evidence_v1(
    record: &MtgoObservedDecisionV1,
    evidence_id: u64,
) -> Result<(&MtgoRectPxV1, &str), String> {
    let evidence = record
        .evidence
        .iter()
        .find(|evidence| evidence.evidence_id == evidence_id)
        .ok_or("visible control cites unknown frame evidence")?;
    let MtgoEvidenceSourceV1::FrameRegion {
        frame_id,
        rect,
        content_sha256,
    } = &evidence.source
    else {
        return Err("visible control evidence is not a pixel region".to_owned());
    };
    if *frame_id != record.frame_id {
        return Err("visible control evidence cites a stale frame".to_owned());
    }
    Ok((rect, content_sha256))
}

fn rects_intersect_v1(left: &MtgoRectPxV1, right: &MtgoRectPxV1) -> bool {
    let Some(left_right) = left.x.checked_add(left.width) else {
        return true;
    };
    let Some(left_bottom) = left.y.checked_add(left.height) else {
        return true;
    };
    let Some(right_right) = right.x.checked_add(right.width) else {
        return true;
    };
    let Some(right_bottom) = right.y.checked_add(right.height) else {
        return true;
    };
    left.x < right_right && left_right > right.x && left.y < right_bottom && left_bottom > right.y
}

fn verify_runtime_artifact_v1(
    path: &Path,
    expected_sha256: &str,
    label: &str,
) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!("{label} path must be absolute"));
    }
    let link_metadata =
        fs::symlink_metadata(path).map_err(|error| format!("inspect {label} metadata: {error}"))?;
    if link_metadata.file_type().is_symlink() || !link_metadata.is_file() {
        return Err(format!("{label} must be a regular non-symlink file"));
    }
    let canonical = fs::canonicalize(path).map_err(|error| format!("resolve {label}: {error}"))?;
    let actual_sha256 = hash_bounded_file_v1(&canonical, label)?;
    if actual_sha256 != expected_sha256 {
        return Err(format!("{label} hash differs from the admitted profile"));
    }
    Ok(canonical)
}

pub(super) fn verify_duel_perception_runtime_identity_now_v1(
    runtime: &OpaqueMtgoVerifiedDuelPerceptionRuntimeV1,
) -> Result<(), String> {
    for (path, expected, label) in [
        (
            runtime.executable_path.as_path(),
            runtime
                .commitments
                .perception_pipeline_binary_sha256
                .as_str(),
            "perception pipeline binary",
        ),
        (
            runtime.classifier_assets_manifest_path.as_path(),
            runtime
                .commitments
                .classifier_assets_manifest_sha256
                .as_str(),
            "classifier assets manifest",
        ),
        (
            runtime.card_database_profile_path.as_path(),
            runtime.commitments.card_database_profile_sha256.as_str(),
            "card database profile",
        ),
    ] {
        if hash_bounded_file_v1(path, label)? != expected {
            return Err(format!("{label} changed after runtime verification"));
        }
    }
    Ok(())
}

fn verify_gesture_target_runtime_identity_now_v1(
    runtime: &OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1,
) -> Result<(), String> {
    for (path, expected, label) in [
        (
            runtime.executable_path.as_path(),
            runtime
                .commitments
                .gesture_target_runtime_binary_sha256
                .as_str(),
            "gesture-target runtime binary",
        ),
        (
            runtime.assets_manifest_path.as_path(),
            runtime
                .commitments
                .gesture_target_assets_manifest_sha256
                .as_str(),
            "gesture-target assets manifest",
        ),
    ] {
        if hash_bounded_file_v1(path, label)? != expected {
            return Err(format!("{label} changed after runtime verification"));
        }
    }
    Ok(())
}

fn hash_bounded_file_v1(path: &Path, label: &str) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| format!("open {label}: {error}"))?;
    let length = file
        .metadata()
        .map_err(|error| format!("inspect {label}: {error}"))?
        .len();
    if length == 0 || length > MAX_RUNTIME_ARTIFACT_BYTES_V1 {
        return Err(format!("{label} length is outside the supported range"));
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("read {label}: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn invoke_verified_perception_process_v1(
    runtime: &OpaqueMtgoVerifiedDuelPerceptionRuntimeV1,
    header_json: &[u8],
    canonical_bgra8: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    invoke_verified_classifier_process_v1(
        runtime,
        "--mtgo-visible-duel-perception-v1",
        DUEL_PERCEPTION_PROTOCOL_MAGIC_V1,
        "duel perception",
        header_json,
        canonical_bgra8,
        timeout,
    )
}

pub(super) fn invoke_verified_competitive_pregame_process_v1(
    runtime: &OpaqueMtgoVerifiedDuelPerceptionRuntimeV1,
    header_json: &[u8],
    canonical_bgra8: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    invoke_verified_classifier_process_v1(
        runtime,
        "--mtgo-visible-competitive-pregame-v1",
        COMPETITIVE_PREGAME_PROTOCOL_MAGIC_V1,
        "competitive pregame",
        header_json,
        canonical_bgra8,
        timeout,
    )
}

pub(super) fn invoke_verified_competitive_pregame_public_context_process_v1(
    runtime: &OpaqueMtgoVerifiedDuelPerceptionRuntimeV1,
    header_json: &[u8],
    canonical_bgra8: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    invoke_verified_classifier_process_v1(
        runtime,
        "--mtgo-visible-competitive-pregame-public-context-v1",
        COMPETITIVE_PREGAME_PUBLIC_CONTEXT_PROTOCOL_MAGIC_V1,
        "competitive pregame public context",
        header_json,
        canonical_bgra8,
        timeout,
    )
}

fn invoke_verified_classifier_process_v1(
    runtime: &OpaqueMtgoVerifiedDuelPerceptionRuntimeV1,
    mode_argument: &str,
    protocol_magic: &[u8],
    protocol_label: &str,
    header_json: &[u8],
    canonical_bgra8: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let mut child = Command::new(&runtime.executable_path)
        .arg(mode_argument)
        .arg("--classifier-assets-manifest")
        .arg(&runtime.classifier_assets_manifest_path)
        .arg("--card-database-profile")
        .arg(&runtime.card_database_profile_path)
        .current_dir(
            runtime
                .executable_path
                .parent()
                .ok_or("perception binary has no parent directory")?,
        )
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("start verified {protocol_label} runtime: {error}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| format!("verified {protocol_label} runtime has no stdin"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("verified {protocol_label} runtime has no stdout"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("verified {protocol_label} runtime has no stderr"))?;
    let started = Instant::now();
    let (status, output, output_truncated, stderr_digest, stderr_truncated) =
        thread::scope(|scope| {
            let writer = scope.spawn(|| -> Result<(), String> {
                stdin
                    .write_all(protocol_magic)
                    .and_then(|_| {
                        stdin.write_all(
                            &u64::try_from(header_json.len())
                                .map_err(|_| {
                                    std::io::Error::new(
                                        std::io::ErrorKind::InvalidInput,
                                        format!("{protocol_label} header is too large"),
                                    )
                                })?
                                .to_be_bytes(),
                        )
                    })
                    .and_then(|_| stdin.write_all(header_json))
                    .and_then(|_| stdin.write_all(canonical_bgra8))
                    .map_err(|error| format!("write {protocol_label} request: {error}"))?;
                drop(stdin);
                Ok(())
            });
            let stdout_reader = scope
                .spawn(|| read_bounded_and_drain_v1(&mut stdout, MAX_PERCEPTION_RESPONSE_BYTES_V1));
            let stderr_reader = scope
                .spawn(|| read_bounded_and_drain_v1(&mut stderr, MAX_PERCEPTION_STDERR_BYTES_V1));
            let status = loop {
                if let Some(status) = child
                    .try_wait()
                    .map_err(|error| format!("poll {protocol_label} runtime: {error}"))?
                {
                    break status;
                }
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("verified {protocol_label} runtime timed out"));
                }
                thread::sleep(Duration::from_millis(5));
            };
            writer
                .join()
                .map_err(|_| format!("{protocol_label} request writer panicked"))??;
            let (output, output_truncated) = stdout_reader
                .join()
                .map_err(|_| format!("{protocol_label} stdout reader panicked"))??;
            let (stderr_bytes, stderr_truncated) = stderr_reader
                .join()
                .map_err(|_| format!("{protocol_label} stderr reader panicked"))??;
            Ok::<_, String>((
                status,
                output,
                output_truncated,
                sha256_hex_v1(&stderr_bytes),
                stderr_truncated,
            ))
        })?;
    validate_process_result_v1(
        status,
        output,
        output_truncated,
        &stderr_digest,
        stderr_truncated,
    )
}

fn invoke_verified_gesture_target_process_v1(
    runtime: &OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1,
    header_json: &[u8],
    canonical_bgra8: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let mut child = Command::new(&runtime.executable_path)
        .arg("--mtgo-visible-duel-gesture-target-v1")
        .arg("--gesture-target-assets-manifest")
        .arg(&runtime.assets_manifest_path)
        .current_dir(
            runtime
                .executable_path
                .parent()
                .ok_or("gesture-target binary has no parent directory")?,
        )
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("start verified duel gesture-target runtime: {error}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or("verified duel gesture-target runtime has no stdin")?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or("verified duel gesture-target runtime has no stdout")?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or("verified duel gesture-target runtime has no stderr")?;
    let started = Instant::now();
    let (status, output, output_truncated, stderr_digest, stderr_truncated) =
        thread::scope(|scope| {
            let writer = scope.spawn(|| -> Result<(), String> {
                stdin
                    .write_all(DUEL_GESTURE_TARGET_PROTOCOL_MAGIC_V1)
                    .and_then(|_| {
                        stdin.write_all(
                            &u64::try_from(header_json.len())
                                .map_err(|_| {
                                    std::io::Error::new(
                                        std::io::ErrorKind::InvalidInput,
                                        "duel gesture-target header is too large",
                                    )
                                })?
                                .to_be_bytes(),
                        )
                    })
                    .and_then(|_| stdin.write_all(header_json))
                    .and_then(|_| stdin.write_all(canonical_bgra8))
                    .map_err(|error| format!("write duel gesture-target request: {error}"))?;
                drop(stdin);
                Ok(())
            });
            let stdout_reader = scope
                .spawn(|| read_bounded_and_drain_v1(&mut stdout, MAX_PERCEPTION_RESPONSE_BYTES_V1));
            let stderr_reader = scope
                .spawn(|| read_bounded_and_drain_v1(&mut stderr, MAX_PERCEPTION_STDERR_BYTES_V1));
            let status = loop {
                if let Some(status) = child
                    .try_wait()
                    .map_err(|error| format!("poll duel gesture-target runtime: {error}"))?
                {
                    break status;
                }
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("verified duel gesture-target runtime timed out".to_owned());
                }
                thread::sleep(Duration::from_millis(5));
            };
            writer
                .join()
                .map_err(|_| "duel gesture-target request writer panicked".to_owned())??;
            let (output, output_truncated) = stdout_reader
                .join()
                .map_err(|_| "duel gesture-target stdout reader panicked".to_owned())??;
            let (stderr_bytes, stderr_truncated) = stderr_reader
                .join()
                .map_err(|_| "duel gesture-target stderr reader panicked".to_owned())??;
            Ok::<_, String>((
                status,
                output,
                output_truncated,
                sha256_hex_v1(&stderr_bytes),
                stderr_truncated,
            ))
        })?;
    validate_gesture_target_process_result_v1(
        status,
        output,
        output_truncated,
        &stderr_digest,
        stderr_truncated,
    )
}

fn read_bounded_and_drain_v1(
    reader: &mut impl Read,
    maximum_retained: usize,
) -> Result<(Vec<u8>, bool), String> {
    let mut retained = Vec::new();
    let mut truncated = false;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("read perception process pipe: {error}"))?;
        if read == 0 {
            break;
        }
        let remaining = maximum_retained.saturating_sub(retained.len());
        let keep = remaining.min(read);
        retained.extend_from_slice(&buffer[..keep]);
        truncated |= keep != read;
    }
    Ok((retained, truncated))
}

fn validate_process_result_v1(
    status: ExitStatus,
    output: Vec<u8>,
    output_truncated: bool,
    stderr_digest: &str,
    stderr_truncated: bool,
) -> Result<Vec<u8>, String> {
    if !status.success() {
        return Err(format!(
            "verified duel perception runtime failed: status={status},stderr_sha256={stderr_digest},stderr_truncated={stderr_truncated}"
        ));
    }
    if output_truncated || output.is_empty() {
        return Err("verified duel perception response is empty or exceeds 16 MiB".to_owned());
    }
    Ok(output)
}

fn validate_gesture_target_process_result_v1(
    status: ExitStatus,
    output: Vec<u8>,
    output_truncated: bool,
    stderr_digest: &str,
    stderr_truncated: bool,
) -> Result<Vec<u8>, String> {
    if !status.success() {
        return Err(format!(
            "verified duel gesture-target runtime failed: status={status},stderr_sha256={stderr_digest},stderr_truncated={stderr_truncated}"
        ));
    }
    if output_truncated || output.is_empty() {
        return Err("verified duel gesture-target response is empty or exceeds 16 MiB".to_owned());
    }
    Ok(output)
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

fn looks_like_lower_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn gesture_transition_requires_exact_changed_target_pixels() {
        let size = MtgoSizePxV1 {
            width: 2,
            height: 1,
        };
        let rect = MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        let before = [1_u8, 2, 3, 255, 5, 6, 7, 255];
        let mut changed_target = before;
        changed_target[0] ^= 1;
        let before_sha = visible_frame_region_content_sha256_v1(&before, &size, &rect).unwrap();
        let changed_sha =
            visible_frame_region_content_sha256_v1(&changed_target, &size, &rect).unwrap();
        let checked = validate_changed_gesture_region_pixels_v1(
            &before,
            &changed_target,
            &size,
            &rect,
            &before_sha,
            &changed_sha,
        )
        .unwrap();
        assert_eq!(checked, (before_sha.clone(), changed_sha));

        assert!(validate_changed_gesture_region_pixels_v1(
            &before,
            &before,
            &size,
            &rect,
            &before_sha,
            &before_sha,
        )
        .is_err());

        let mut changed_unrelated = before;
        changed_unrelated[4] ^= 1;
        assert!(validate_changed_gesture_region_pixels_v1(
            &before,
            &changed_unrelated,
            &size,
            &rect,
            &before_sha,
            &before_sha,
        )
        .is_err());
        assert!(validate_changed_gesture_region_pixels_v1(
            &before,
            &changed_target,
            &size,
            &rect,
            &"0".repeat(64),
            &visible_frame_region_content_sha256_v1(&changed_target, &size, &rect).unwrap(),
        )
        .is_err());
    }

    #[test]
    fn competitive_launch_title_parser_requires_exact_visible_identity() {
        assert_eq!(
            parse_competitive_duel_window_title_v1(
                "(1-on-1): Modern: Vs. VisibleOpponent Match #123 - Game #456",
                "Modern",
            )
            .unwrap(),
            (
                "VisibleOpponent".to_owned(),
                "123".to_owned(),
                "456".to_owned(),
            )
        );
        for title in [
            "(1-on-1): Legacy: Vs. VisibleOpponent Match #123 - Game #456",
            "(1-on-1): Modern: Vs. VisibleOpponent",
            "(1-on-1): Modern: Vs. A, B Match #123 - Game #456",
            "(1-on-1): Modern: Vs. VisibleOpponent Match #abc - Game #456",
            "(1-on-1): Modern: Vs. VisibleOpponent Match #123 - Game #xyz",
        ] {
            assert!(parse_competitive_duel_window_title_v1(title, "Modern").is_err());
        }
        assert!(validate_competitive_launch_display_label_v1(
            "\u{202e}spoof",
            64,
            "opponent display name"
        )
        .is_err());
        let outer = MtgoRectPxV1 {
            x: 10,
            y: 20,
            width: 100,
            height: 50,
        };
        assert!(rect_contains_rect_v1(
            &outer,
            &MtgoRectPxV1 {
                x: 15,
                y: 25,
                width: 8,
                height: 8,
            }
        )
        .unwrap());
        assert!(!rect_contains_rect_v1(
            &outer,
            &MtgoRectPxV1 {
                x: 105,
                y: 25,
                width: 8,
                height: 8,
            }
        )
        .unwrap());
    }

    #[test]
    fn competitive_lifecycle_is_bound_to_the_exact_duel_classifier_frame() {
        let pixels = [1_u8, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255];
        let size = MtgoSizePxV1 {
            width: 3,
            height: 1,
        };
        let fact = |kind, rect: MtgoRectPxV1| mtgo_blackbox_v1::MtgoLifecycleVisibleFactV1 {
            kind,
            content_sha256: visible_frame_region_content_sha256_v1(&pixels, &size, &rect).unwrap(),
            rect_client_px: rect,
            confidence_bps: 10_000,
        };
        let snapshot = MtgoVisibleCompetitiveLifecycleSnapshotV1 {
            schema_version: 1,
            snapshot_id: "classifier-bound-duel-lifecycle-v1".to_owned(),
            event_kind: MtgoCompetitiveEventKindV1::League,
            phase: MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
            frame_id: 17,
            frame_sequence: 23,
            frame_sha256: sha256_hex_v1(&pixels),
            client_bounds: MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: 3,
                height: 1,
            },
            event_identity_sha256: Some("1".repeat(64)),
            match_identity_sha256: Some("2".repeat(64)),
            game_number: Some(1),
            entry_terms: None,
            visible_state_complete: true,
            facts: vec![
                fact(
                    MtgoLifecycleVisibleFactKindV1::MatchSurfaceVisible,
                    MtgoRectPxV1 {
                        x: 0,
                        y: 0,
                        width: 3,
                        height: 1,
                    },
                ),
                fact(
                    MtgoLifecycleVisibleFactKindV1::LocalClockVisible,
                    MtgoRectPxV1 {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    },
                ),
                fact(
                    MtgoLifecycleVisibleFactKindV1::OpponentClockVisible,
                    MtgoRectPxV1 {
                        x: 2,
                        y: 0,
                        width: 1,
                        height: 1,
                    },
                ),
            ],
        };
        let checked = validate_visible_competitive_lifecycle_snapshot_v1(snapshot).unwrap();
        let identity = MtgoDuelPerceptionFrameIdentityV1 {
            frame_id: 17,
            frame_sequence: 23,
        };
        validate_competitive_lifecycle_against_duel_pixels_v1(
            &checked,
            identity,
            &sha256_hex_v1(&pixels),
            &size,
            &pixels,
        )
        .unwrap();

        let mut changed_pixels = pixels;
        changed_pixels[0] ^= 1;
        assert!(validate_competitive_lifecycle_against_duel_pixels_v1(
            &checked,
            identity,
            &sha256_hex_v1(&pixels),
            &size,
            &changed_pixels,
        )
        .is_err());
        assert!(validate_competitive_lifecycle_against_duel_pixels_v1(
            &checked,
            MtgoDuelPerceptionFrameIdentityV1 {
                frame_id: 17,
                frame_sequence: 24,
            },
            &sha256_hex_v1(&pixels),
            &size,
            &pixels,
        )
        .is_err());
    }

    fn request_header_v1(pixels: &[u8]) -> MtgoDuelPerceptionRequestHeaderV1 {
        MtgoDuelPerceptionRequestHeaderV1 {
            schema_version: 1,
            protocol: "mtgo_visible_duel_perception_v1".to_owned(),
            frame_id: 7,
            frame_sequence: 11,
            canonical_width: 2,
            canonical_height: 1,
            canonical_stride: 8,
            canonical_byte_length: 8,
            canonical_bgra8_sha256: sha256_hex_v1(pixels),
            source_manifest_sha256: "0".repeat(64),
            source_capture_commitment_sha256: "1".repeat(64),
            source_frame_profile_binding_sha256: "2".repeat(64),
            perception_profile_commitment_sha256: "3".repeat(64),
            perception_profile_admission_commitment_sha256: "4".repeat(64),
            runtime_identity_commitment_sha256: "5".repeat(64),
            perception_pipeline_binary_sha256: "6".repeat(64),
            classifier_assets_manifest_sha256: "7".repeat(64),
            card_database_profile_sha256: "8".repeat(64),
        }
    }

    fn gesture_target_request_header_v1(pixels: &[u8]) -> MtgoDuelGestureTargetRequestHeaderV1 {
        MtgoDuelGestureTargetRequestHeaderV1 {
            schema_version: 1,
            protocol: "mtgo_visible_duel_gesture_target_v1".to_owned(),
            frame_id: 17,
            frame_sequence: 23,
            canonical_width: 2,
            canonical_height: 1,
            canonical_stride: 8,
            canonical_byte_length: 8,
            canonical_bgra8_sha256: sha256_hex_v1(pixels),
            source_capture_commitment_sha256: "1".repeat(64),
            perception_result_commitment_sha256: "2".repeat(64),
            decision_commitment_sha256: "3".repeat(64),
            gesture_plan_commitment_sha256: "4".repeat(64),
            selected_action_family: MtgoDuelActionFamilyV1::PriorityPass,
            stage_index: 0,
            primitive: MtgoDuelGesturePrimitiveV1::ActivatePrimary {
                activation: mtgo_blackbox_v1::MtgoDuelPrimaryActivationV1::SingleLeftClick,
            },
            gesture_evaluation_commitment_sha256: "5".repeat(64),
            gesture_profile_admission_commitment_sha256: "6".repeat(64),
            runtime_identity_commitment_sha256: "7".repeat(64),
            gesture_target_runtime_binary_sha256: "8".repeat(64),
            gesture_target_assets_manifest_sha256: "9".repeat(64),
        }
    }

    #[test]
    fn shared_request_checker_binds_canonical_header_and_every_pixel() {
        let pixels = [1_u8, 2, 3, 4, 5, 6, 7, 8];
        let header = request_header_v1(&pixels);
        let encoded = serde_json::to_vec(&header).unwrap();
        let checked = check_untrusted_duel_perception_request_v1(&encoded, &pixels).unwrap();
        assert_eq!(checked.header_v1(), &header);
        assert_eq!(checked.request_commitment_sha256_v1().len(), 64);
        assert!(!checked.grants_capture_authority_v1());
        assert!(!checked.safe_for_input_v1());

        let mut changed_pixels = pixels;
        changed_pixels[0] ^= 1;
        assert!(check_untrusted_duel_perception_request_v1(&encoded, &changed_pixels).is_err());
        let padded = [&[b' '][..], encoded.as_slice()].concat();
        assert!(check_untrusted_duel_perception_request_v1(&padded, &pixels).is_err());
        let mut wrong_stride = header.clone();
        wrong_stride.canonical_stride = 4;
        assert!(check_untrusted_duel_perception_request_v1(
            &serde_json::to_vec(&wrong_stride).unwrap(),
            &pixels
        )
        .is_err());
    }

    #[test]
    fn gesture_target_request_checker_binds_exact_stage_runtime_and_pixels() {
        let pixels = [1_u8, 2, 3, 255, 5, 6, 7, 255];
        let header = gesture_target_request_header_v1(&pixels);
        let encoded = serde_json::to_vec(&header).unwrap();
        let checked = check_untrusted_duel_gesture_target_request_v1(&encoded, &pixels).unwrap();
        assert_eq!(checked.header_v1(), &header);
        assert_eq!(checked.request_commitment_sha256_v1().len(), 64);
        assert!(!checked.safe_for_input_v1());

        let mut changed_pixels = pixels;
        changed_pixels[0] ^= 1;
        assert!(check_untrusted_duel_gesture_target_request_v1(&encoded, &changed_pixels).is_err());
        let noncanonical = [encoded.as_slice(), &[b' '][..]].concat();
        assert!(check_untrusted_duel_gesture_target_request_v1(&noncanonical, &pixels).is_err());
        let mut wrong_stage = header.clone();
        wrong_stage.stage_index = 32;
        assert!(check_untrusted_duel_gesture_target_request_v1(
            &serde_json::to_vec(&wrong_stage).unwrap(),
            &pixels,
        )
        .is_err());
        let mut invalid_runtime = header;
        invalid_runtime.runtime_identity_commitment_sha256 = "A".repeat(64);
        assert!(check_untrusted_duel_gesture_target_request_v1(
            &serde_json::to_vec(&invalid_runtime).unwrap(),
            &pixels,
        )
        .is_err());
    }

    #[test]
    fn reconstruction_audit_regions_must_match_retained_opaque_pixels() {
        let pixels = [1_u8, 2, 3, 255, 5, 6, 7, 255];
        let size = MtgoSizePxV1 {
            width: 2,
            height: 1,
        };
        let rect = MtgoRectPxV1 {
            x: 1,
            y: 0,
            width: 1,
            height: 1,
        };
        let audit = MtgoObservationReconstructionAuditV1 {
            schema_version: 1,
            audit_id: "opaque-pixel-binding-test-v1".to_owned(),
            topology: mtgo_blackbox_v1::MtgoReconstructionTopologyV1::TwoPlayerDuel,
            frame: mtgo_blackbox_v1::MtgoCalibrationFrameReferenceV1 {
                sequence: 1,
                manifest_sha256: "a".repeat(64),
                frame_sha256: sha256_hex_v1(&pixels),
                client_size_px: size.clone(),
                artifact_kind: mtgo_blackbox_v1::MtgoCalibrationPreviewKindV1::ActingPlayerDuelGameplayCalibrationPreviewV1,
                capture_role: mtgo_blackbox_v1::MtgoCalibrationCaptureRoleV1::ActingPlayerDuel,
                status: mtgo_blackbox_v1::MtgoCalibrationPreviewStatusV1::PendingVisualReview,
                safe_for_semantic_evidence: false,
                safe_for_ocr: false,
                safe_for_policy_scoring: false,
                safe_for_input: false,
            },
            groups: vec![
                mtgo_blackbox_v1::MtgoObservationReconstructionGroupAuditV1 {
                    group: mtgo_blackbox_v1::MtgoObservationReconstructionGroupV1::DuelParticipants,
                    status: mtgo_blackbox_v1::MtgoReconstructionStatusV1::VisibleComplete,
                    visible_regions: vec![mtgo_blackbox_v1::MtgoVisibleRegionCommitmentV1 {
                        rect_client_px: rect.clone(),
                        bgra8_sha256: visible_frame_region_content_sha256_v1(
                            &pixels, &size, &rect,
                        )
                        .unwrap(),
                    }],
                    missing_reason_codes: Vec::new(),
                },
            ],
            observation_complete: false,
            legal_action_set_complete: false,
            ready_for_model_scoring: false,
        };
        validate_reconstruction_audit_pixels_v1(&audit, &pixels, &size).unwrap();

        let mut changed = pixels;
        changed[4] ^= 1;
        assert!(validate_reconstruction_audit_pixels_v1(&audit, &changed, &size).is_err());
    }

    #[test]
    fn immediate_frame_identity_is_capture_bound_nonzero_and_distinct() {
        let commitment = "0123456789abcdef".to_owned() + &"0".repeat(48);
        let baseline = frame_id_from_capture_commitment_v1(&commitment, 99).unwrap();
        assert_eq!(baseline, 0x0123_4567_89ab_cdef);
        assert_eq!(
            frame_id_from_capture_commitment_v1(&commitment, baseline).unwrap(),
            baseline ^ 0xa5a5_5a5a_d3d3_3c3c
        );
        assert_ne!(
            baseline,
            frame_id_from_capture_commitment_v1(
                &("1123456789abcdef".to_owned() + &"0".repeat(48)),
                99,
            )
            .unwrap()
        );
        assert!(frame_id_from_capture_commitment_v1("not-a-hash", 99).is_err());
    }

    #[test]
    fn competitive_bridge_rejects_every_opaque_lineage_substitution() {
        let semantic = serde_json::json!({"kind":"pass","actor":"p0"});
        let semantic_json = serde_json::to_vec(&semantic).unwrap();
        let opaque = OpaqueDuelActionBindingViewV1 {
            opaque_control_resolution_commitment_sha256: "0".repeat(64),
            profile_bound_resolution_commitment_sha256: "9".repeat(64),
            decision_commitment_sha256: "1".repeat(64),
            selection_commitment_sha256: "2".repeat(64),
            control_resolution_commitment_sha256: "3".repeat(64),
            perception_profile_admission_commitment_sha256: "4".repeat(64),
            deployment_commitment_sha256: "5".repeat(64),
            control_id: "priority-pass".to_owned(),
            frame_id: 7,
            frame_sequence: 11,
            source_manifest_sha256: "6".repeat(64),
            source_frame_sha256: "7".repeat(64),
            source_output_identity_sha256: "8".repeat(64),
            source_client_size_px: MtgoSizePxV1 {
                width: 1240,
                height: 740,
            },
            selected_semantic_json: semantic_json,
        };
        let baseline = mtgo_blackbox_v1::MtgoProfileBoundActionPostconditionPlanCommitmentsV1 {
            profile_bound_resolution_commitment_sha256: "9".repeat(64),
            decision_commitment_sha256: opaque.decision_commitment_sha256.clone(),
            selection_commitment_sha256: opaque.selection_commitment_sha256.clone(),
            control_resolution_commitment_sha256: opaque
                .control_resolution_commitment_sha256
                .clone(),
            source_candidate_commitment_sha256: "a".repeat(64),
            perception_profile_admission_commitment_sha256: opaque
                .perception_profile_admission_commitment_sha256
                .clone(),
            deployment_commitment_sha256: opaque.deployment_commitment_sha256.clone(),
            control_id: opaque.control_id.clone(),
            frame_id: opaque.frame_id,
            frame_sequence: opaque.frame_sequence,
            source_manifest_sha256: opaque.source_manifest_sha256.clone(),
            source_frame_sha256: opaque.source_frame_sha256.clone(),
            source_output_identity_sha256: opaque.source_output_identity_sha256.clone(),
            source_client_size_px: opaque.source_client_size_px.clone(),
            selected_semantic_sha256: selected_semantic_sha256_v1(&opaque.selected_semantic_json),
            plan_commitment_sha256: "b".repeat(64),
        };
        validate_opaque_competitive_action_binding_v1(&opaque, &baseline).unwrap();

        for mutation in 0..13 {
            let mut changed = baseline.clone();
            match mutation {
                0 => changed.decision_commitment_sha256 = "c".repeat(64),
                1 => changed.selection_commitment_sha256 = "c".repeat(64),
                2 => changed.control_resolution_commitment_sha256 = "c".repeat(64),
                3 => changed.perception_profile_admission_commitment_sha256 = "c".repeat(64),
                4 => changed.deployment_commitment_sha256 = "c".repeat(64),
                5 => changed.control_id = "other-control".to_owned(),
                6 => changed.frame_id += 1,
                7 => changed.frame_sequence += 1,
                8 => changed.source_manifest_sha256 = "c".repeat(64),
                9 => changed.source_frame_sha256 = "c".repeat(64),
                10 => changed.source_output_identity_sha256 = "c".repeat(64),
                11 => changed.source_client_size_px.width += 1,
                12 => changed.selected_semantic_sha256 = "c".repeat(64),
                _ => unreachable!(),
            }
            assert!(validate_opaque_competitive_action_binding_v1(&opaque, &changed).is_err());
        }
        let mut other_semantic = opaque.clone();
        other_semantic.selected_semantic_json =
            serde_json::to_vec(&serde_json::json!({"kind":"pass","actor":"p1"})).unwrap();
        assert!(validate_opaque_competitive_action_binding_v1(&other_semantic, &baseline).is_err());
    }

    #[test]
    fn runtime_and_result_commitments_change_with_every_identity_layer() {
        let baseline = commitment_v1(
            DUEL_PERCEPTION_RESULT_DOMAIN_V1,
            &[b"frame", b"runtime", b"request", b"decision", b"scope"],
        );
        assert_eq!(baseline.len(), 64);
        for replacement in [
            [
                b"other-frame".as_slice(),
                b"runtime",
                b"request",
                b"decision",
                b"scope",
            ],
            [
                b"frame".as_slice(),
                b"other-runtime",
                b"request",
                b"decision",
                b"scope",
            ],
            [
                b"frame".as_slice(),
                b"runtime",
                b"other-request",
                b"decision",
                b"scope",
            ],
            [
                b"frame".as_slice(),
                b"runtime",
                b"request",
                b"other-decision",
                b"scope",
            ],
        ] {
            assert_ne!(
                baseline,
                commitment_v1(DUEL_PERCEPTION_RESULT_DOMAIN_V1, &replacement)
            );
        }
    }

    #[test]
    fn bounded_reader_drains_but_retains_only_the_limit() {
        let bytes = vec![7_u8; 1024];
        let (retained, truncated) = read_bounded_and_drain_v1(&mut bytes.as_slice(), 100).unwrap();
        assert_eq!(retained, vec![7_u8; 100]);
        assert!(truncated);
    }

    #[test]
    fn runtime_artifact_verification_requires_absolute_exact_nonempty_file() {
        let root = std::env::temp_dir().join(format!(
            "mtgo-duel-perception-runtime-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let artifact = root.join("classifier.exe");
        fs::write(&artifact, b"reviewed-classifier-bytes-v1").unwrap();
        let expected = sha256_hex_v1(b"reviewed-classifier-bytes-v1");
        assert_eq!(
            verify_runtime_artifact_v1(&artifact, &expected, "test artifact").unwrap(),
            fs::canonicalize(&artifact).unwrap()
        );
        assert!(
            verify_runtime_artifact_v1(&artifact, &"0".repeat(64), "test artifact")
                .unwrap_err()
                .contains("hash differs")
        );
        assert!(verify_runtime_artifact_v1(
            Path::new("relative-classifier.exe"),
            &expected,
            "test artifact"
        )
        .unwrap_err()
        .contains("absolute"));
        let empty = root.join("empty.exe");
        fs::write(&empty, []).unwrap();
        assert!(hash_bounded_file_v1(&empty, "empty artifact")
            .unwrap_err()
            .contains("length"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn process_result_never_returns_truncated_or_failed_output() {
        let successful = synthetic_exit_status_v1(0);
        assert_eq!(
            validate_process_result_v1(
                successful,
                br#"{"schema_version":1}"#.to_vec(),
                false,
                &"0".repeat(64),
                false,
            )
            .unwrap(),
            br#"{"schema_version":1}"#
        );
        assert!(validate_process_result_v1(
            synthetic_exit_status_v1(0),
            Vec::new(),
            false,
            &"0".repeat(64),
            false,
        )
        .is_err());
        assert!(validate_process_result_v1(
            synthetic_exit_status_v1(0),
            vec![1],
            true,
            &"0".repeat(64),
            false,
        )
        .is_err());
        assert!(validate_process_result_v1(
            synthetic_exit_status_v1(9),
            vec![1],
            false,
            &"0".repeat(64),
            true,
        )
        .is_err());
        let (retained, truncated) =
            read_bounded_and_drain_v1(&mut Cursor::new(vec![3_u8; 10]), 10).unwrap();
        assert_eq!(retained.len(), 10);
        assert!(!truncated);
    }

    #[cfg(windows)]
    fn synthetic_exit_status_v1(code: u32) -> ExitStatus {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(code)
    }
}
