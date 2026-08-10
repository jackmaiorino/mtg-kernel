use super::{
    serialize_manifest_v2, sha256_hex_v1, MtgoAdmittedDuelVisibleFrameCommitmentsV1,
    OpaqueMtgoAdmittedDuelVisibleFrameV1,
};
use mtgo_blackbox_v1::{
    duel_action_family_v1, model_deployment_commitment_v1, preview_output_identity_commitment_v1,
    resolve_selected_visible_control_v1, score_and_select_external_model_v1,
    validate_observed_decision_v1, visible_frame_region_content_sha256_v1,
    AdmittedMtgoDuelPerceptionProfileV1, CheckedUntrustedMtgoCompetitiveGameplayActionPlanV1,
    CheckedUntrustedMtgoModelSelectionV1, CheckedUntrustedMtgoResolvedActionControlV1,
    MtgoCompetitiveEventKindV1, MtgoEvidenceSourceV1, MtgoExpectedModelDeploymentV1,
    MtgoExternalObservationScorerV1, MtgoObservedDecisionV1, MtgoRectPxV1,
    MtgoSignedRectDesktopPxV1, MtgoSizePxV1, MtgoVisibleActionControlSetV1,
    ValidatedMtgoObservedDecisionV1, MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1,
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
const DUEL_PERCEPTION_RESULT_DOMAIN_V1: &[u8] = b"mtgo-duel-perception-result-v1";
const DUEL_OPAQUE_MODEL_SELECTION_DOMAIN_V1: &[u8] = b"mtgo-opaque-duel-model-selection-v1";
const DUEL_OPAQUE_CONTROL_RESOLUTION_DOMAIN_V1: &[u8] = b"mtgo-opaque-duel-control-resolution-v1";
const DUEL_OPAQUE_COMPETITIVE_ACTION_PLAN_DOMAIN_V1: &[u8] =
    b"mtgo-opaque-competitive-duel-action-plan-v1";
const DUEL_PERCEPTION_PROTOCOL_MAGIC_V1: &[u8] = b"MTGO_VISIBLE_DUEL_PERCEPTION_V1\0";
const MAX_RUNTIME_ARTIFACT_BYTES_V1: u64 = 512 * 1024 * 1024;
const MAX_PERCEPTION_RESPONSE_BYTES_V1: usize = 16 * 1024 * 1024;
const MAX_PERCEPTION_STDERR_BYTES_V1: usize = 64 * 1024;

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
    pub decision: MtgoObservedDecisionV1,
    pub visible_controls: MtgoVisibleActionControlSetV1,
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
    pub source_capture_commitment_sha256: String,
    pub source_frame_profile_binding_sha256: String,
    pub perception_profile_commitment_sha256: String,
    pub perception_profile_admission_commitment_sha256: String,
    pub runtime_identity_commitment_sha256: String,
    pub perception_pipeline_binary_sha256: String,
    pub classifier_assets_manifest_sha256: String,
    pub card_database_profile_sha256: String,
}

/// Structurally checked protocol request metadata. This does not attest that a
/// caller owns a DXGI frame and it retains no pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedUntrustedMtgoDuelPerceptionRequestV1 {
    header: MtgoDuelPerceptionRequestHeaderV1,
    request_commitment_sha256: String,
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
    pub(super) decision_record: MtgoObservedDecisionV1,
    pub(super) visible_controls: MtgoVisibleActionControlSetV1,
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
}

/// Copyable scoring telemetry without an observation, semantic, coordinate, or
/// input conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoOpaqueDuelModelSelectionCommitmentsV1 {
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
pub struct OpaqueMtgoProfileBoundDuelModelSelectionV1 {
    pub(super) perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    pub(super) selection: CheckedUntrustedMtgoModelSelectionV1,
    deployment_commitment_sha256: String,
    opaque_selection_commitment_sha256: String,
}

impl OpaqueMtgoProfileBoundDuelModelSelectionV1 {
    pub fn commitments_v1(&self) -> MtgoOpaqueDuelModelSelectionCommitmentsV1 {
        MtgoOpaqueDuelModelSelectionCommitmentsV1 {
            perception_result_commitment_sha256: self
                .perception
                .perception_result_commitment_sha256
                .clone(),
            decision_commitment_sha256: self
                .perception
                .validated_decision
                .decision_commitment_sha256()
                .to_owned(),
            deployment_commitment_sha256: self.deployment_commitment_sha256.clone(),
            selection_commitment_sha256: self.selection.selection_commitment_sha256().to_owned(),
            opaque_selection_commitment_sha256: self.opaque_selection_commitment_sha256.clone(),
            selected_index: self.selection.selected_index(),
            selected_logit_f32_bits: self.selection.selected_logit_f32_bits(),
            value_f32_bits: self.selection.value_f32_bits(),
        }
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
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
    resolved: CheckedUntrustedMtgoResolvedActionControlV1,
    #[allow(dead_code)]
    pub(super) rect_client_px: MtgoRectPxV1,
    opaque_control_resolution_commitment_sha256: String,
}

impl OpaqueMtgoProfileBoundDuelResolvedControlV1 {
    pub fn commitments_v1(&self) -> MtgoOpaqueDuelResolvedControlCommitmentsV1 {
        MtgoOpaqueDuelResolvedControlCommitmentsV1 {
            opaque_selection_commitment_sha256: self
                .selection
                .opaque_selection_commitment_sha256
                .clone(),
            decision_commitment_sha256: self.resolved.decision_commitment_sha256().to_owned(),
            selection_commitment_sha256: self.resolved.selection_commitment_sha256().to_owned(),
            control_resolution_commitment_sha256: self
                .resolved
                .resolution_commitment_sha256()
                .to_owned(),
            opaque_control_resolution_commitment_sha256: self
                .opaque_control_resolution_commitment_sha256
                .clone(),
            control_id: self.resolved.control_id().to_owned(),
            frame_id: self.resolved.frame_id(),
            frame_sequence: self.resolved.frame_sequence(),
        }
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

/// Coordinate-free telemetry for an exact opaque Windows control joined to a
/// separately checked competitive authorization and visible postcondition
/// plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoOpaqueCompetitiveDuelActionPlanCommitmentsV1 {
    pub opaque_control_resolution_commitment_sha256: String,
    pub postcondition_plan_commitment_sha256: String,
    pub competitive_scope_commitment_sha256: String,
    pub competitive_authorization_commitment_sha256: String,
    pub opaque_competitive_action_plan_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
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
    competitive: CheckedUntrustedMtgoCompetitiveGameplayActionPlanV1,
    opaque_competitive_action_plan_commitment_sha256: String,
}

impl OpaqueMtgoCompetitiveDuelActionPlanV1 {
    pub fn commitments_v1(&self) -> MtgoOpaqueCompetitiveDuelActionPlanCommitmentsV1 {
        let control = self.control.commitments_v1();
        let postcondition = self.competitive.postcondition_plan_commitments_v1();
        MtgoOpaqueCompetitiveDuelActionPlanCommitmentsV1 {
            opaque_control_resolution_commitment_sha256: control
                .opaque_control_resolution_commitment_sha256,
            postcondition_plan_commitment_sha256: postcondition.plan_commitment_sha256,
            competitive_scope_commitment_sha256: self
                .competitive
                .competitive_scope_commitment_sha256()
                .to_owned(),
            competitive_authorization_commitment_sha256: self
                .competitive
                .authorization_commitment_sha256()
                .to_owned(),
            opaque_competitive_action_plan_commitment_sha256: self
                .opaque_competitive_action_plan_commitment_sha256
                .clone(),
            event_kind: self.competitive.event_kind(),
            game_number: self.competitive.game_number(),
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
    verify_runtime_identity_now_v1(runtime)?;

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
    verify_runtime_identity_now_v1(runtime)?;
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
            b"opaque_source_retained_no_input_or_event_entry_authority",
        ],
    );
    Ok(OpaqueMtgoAdmittedDuelPerceptionV1 {
        source_frame,
        validated_decision: validated,
        decision_record: response.decision,
        visible_controls: response.visible_controls,
        runtime_identity_commitment_sha256: runtime
            .commitments
            .runtime_identity_commitment_sha256
            .clone(),
        request_commitment_sha256,
        perception_result_commitment_sha256,
    })
}

pub fn score_and_select_opaque_admitted_duel_perception_v1<S: MtgoExternalObservationScorerV1>(
    perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    deployment: &MtgoExpectedModelDeploymentV1,
    scorer: &mut S,
) -> Result<OpaqueMtgoProfileBoundDuelModelSelectionV1, String> {
    let source = perception.source_frame.commitments_v1();
    if source.perception_profile_commitment_sha256 != profile.perception_profile_commitment_sha256()
        || source.perception_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
    {
        return Err("opaque perception and admitted profile differ at scoring".to_owned());
    }
    let deployment_commitment_sha256 = model_deployment_commitment_v1(deployment)
        .map_err(|error| format!("invalid model deployment: {error}"))?;
    let selection =
        score_and_select_external_model_v1(&perception.validated_decision, deployment, scorer)
            .map_err(|error| format!("duel model scoring failed: {error}"))?;
    if selection.decision_commitment_sha256()
        != perception.validated_decision.decision_commitment_sha256()
        || selection.selected_index() >= perception.validated_decision.legal_actions().len()
    {
        return Err("duel model selection does not bind the retained decision".to_owned());
    }
    let selected_semantic = serde_json::to_vec(
        &perception.validated_decision.legal_actions()[selection.selected_index()],
    )
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
            &selected_semantic,
            b"opaque_source_retained_no_input_or_event_entry_authority",
        ],
    );
    Ok(OpaqueMtgoProfileBoundDuelModelSelectionV1 {
        perception,
        selection,
        deployment_commitment_sha256,
        opaque_selection_commitment_sha256,
    })
}

/// Resolves the selected semantic against the complete control set emitted by
/// the same exact classifier invocation. The existing black-box resolver
/// rechecks decision, frame, prompt, enabled state, confidence, legality, and
/// unique selected match. This Windows-side wrapper additionally retains the
/// opaque source frame and its private pixel-region coordinates.
pub fn resolve_opaque_profile_bound_duel_control_v1(
    selection: OpaqueMtgoProfileBoundDuelModelSelectionV1,
) -> Result<OpaqueMtgoProfileBoundDuelResolvedControlV1, String> {
    let selected_semantic = selection
        .perception
        .validated_decision
        .legal_actions()
        .get(selection.selection.selected_index())
        .ok_or("selected duel action is outside the retained legal-action vector")?;
    let selected_candidate = selection
        .perception
        .visible_controls
        .controls
        .iter()
        .find(|candidate| &candidate.semantic == selected_semantic)
        .ok_or("selected duel action has no retained visible control")?;
    let selected_control_id = selected_candidate.control_id.clone();
    let selected_evidence_id = selected_candidate.frame_region_evidence_id;
    let selected_semantic_json = serde_json::to_vec(selected_semantic)
        .map_err(|error| format!("serialize selected duel semantic: {error}"))?;
    let (rect_client_px, region_content_sha256) =
        frame_region_for_evidence_v1(&selection.perception.decision_record, selected_evidence_id)?;
    let rect_client_px = rect_client_px.clone();
    let region_content_sha256 = region_content_sha256.to_owned();
    let resolved = resolve_selected_visible_control_v1(
        &selection.perception.validated_decision,
        &selection.selection,
        selection.perception.visible_controls.clone(),
    )
    .map_err(|error| format!("visible duel control resolution failed: {error}"))?;
    if resolved.control_id() != selected_control_id
        || resolved.frame_id() != selection.perception.validated_decision.frame_id()
        || resolved.frame_sequence() != selection.perception.validated_decision.frame_sequence()
        || resolved.selection_commitment_sha256()
            != selection.selection.selection_commitment_sha256()
    {
        return Err("resolved duel control does not retain the exact opaque selection".to_owned());
    }
    let rect_json = serde_json::to_vec(&rect_client_px)
        .map_err(|error| format!("serialize resolved duel control rectangle: {error}"))?;
    let opaque_control_resolution_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_CONTROL_RESOLUTION_DOMAIN_V1,
        &[
            selection.opaque_selection_commitment_sha256.as_bytes(),
            resolved.resolution_commitment_sha256().as_bytes(),
            selected_control_id.as_bytes(),
            &selected_semantic_json,
            &rect_json,
            region_content_sha256.as_bytes(),
            b"opaque_source_retained_coordinates_private_no_input_or_event_entry_authority",
        ],
    );
    Ok(OpaqueMtgoProfileBoundDuelResolvedControlV1 {
        selection,
        resolved,
        rect_client_px,
        opaque_control_resolution_commitment_sha256,
    })
}

/// Joins the opaque Windows capture-to-control chain to the separately checked
/// competitive postcondition and authorization plan. Every shared identity is
/// compared before either move-only input is retained.
pub fn bind_opaque_duel_control_to_competitive_action_plan_v1(
    control: OpaqueMtgoProfileBoundDuelResolvedControlV1,
    competitive: CheckedUntrustedMtgoCompetitiveGameplayActionPlanV1,
) -> Result<OpaqueMtgoCompetitiveDuelActionPlanV1, String> {
    let opaque = opaque_duel_action_binding_view_v1(&control)?;
    let postcondition = competitive.postcondition_plan_commitments_v1();
    validate_opaque_competitive_action_binding_v1(
        &opaque,
        &postcondition,
        competitive.selected_semantic(),
    )?;

    let selected_semantic_json = serde_json::to_vec(competitive.selected_semantic())
        .map_err(|error| format!("serialize competitive duel semantic: {error}"))?;
    let opaque_competitive_action_plan_commitment_sha256 = commitment_v1(
        DUEL_OPAQUE_COMPETITIVE_ACTION_PLAN_DOMAIN_V1,
        &[
            opaque
                .opaque_control_resolution_commitment_sha256
                .as_bytes(),
            postcondition.plan_commitment_sha256.as_bytes(),
            competitive.competitive_scope_commitment_sha256().as_bytes(),
            competitive.authorization_commitment_sha256().as_bytes(),
            &selected_semantic_json,
            b"opaque_coordinates_and_postconditions_retained_no_input_or_event_entry_authority",
        ],
    );
    Ok(OpaqueMtgoCompetitiveDuelActionPlanV1 {
        control,
        competitive,
        opaque_competitive_action_plan_commitment_sha256,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpaqueDuelActionBindingViewV1 {
    opaque_control_resolution_commitment_sha256: String,
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
    let selected_semantic = perception
        .validated_decision
        .legal_actions()
        .get(control.selection.selection.selected_index())
        .ok_or("opaque duel selected semantic is outside the retained decision")?;
    let selected_semantic_json = serde_json::to_vec(selected_semantic)
        .map_err(|error| format!("serialize opaque duel selected semantic: {error}"))?;
    Ok(OpaqueDuelActionBindingViewV1 {
        opaque_control_resolution_commitment_sha256: commitments
            .opaque_control_resolution_commitment_sha256,
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
        selected_semantic_json,
    })
}

fn validate_opaque_competitive_action_binding_v1<T: Serialize + ?Sized>(
    opaque: &OpaqueDuelActionBindingViewV1,
    postcondition: &mtgo_blackbox_v1::MtgoProfileBoundActionPostconditionPlanCommitmentsV1,
    competitive_semantic: &T,
) -> Result<(), String> {
    let competitive_semantic_json = serde_json::to_vec(competitive_semantic)
        .map_err(|error| format!("serialize checked competitive semantic: {error}"))?;
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
        || opaque.selected_semantic_json != competitive_semantic_json
    {
        return Err(
            "competitive duel plan does not bind the exact opaque source, model selection, and visible control"
                .to_owned(),
        );
    }
    Ok(())
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

fn verify_runtime_identity_now_v1(
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
    let mut child = Command::new(&runtime.executable_path)
        .arg("--mtgo-visible-duel-perception-v1")
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
        .map_err(|error| format!("start verified duel perception runtime: {error}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or("verified duel perception runtime has no stdin")?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or("verified duel perception runtime has no stdout")?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or("verified duel perception runtime has no stderr")?;
    let started = Instant::now();
    let (status, output, output_truncated, stderr_digest, stderr_truncated) =
        thread::scope(|scope| {
            let writer = scope.spawn(|| -> Result<(), String> {
                stdin
                    .write_all(DUEL_PERCEPTION_PROTOCOL_MAGIC_V1)
                    .and_then(|_| {
                        stdin.write_all(
                            &u64::try_from(header_json.len())
                                .map_err(|_| {
                                    std::io::Error::new(
                                        std::io::ErrorKind::InvalidInput,
                                        "duel perception header is too large",
                                    )
                                })?
                                .to_be_bytes(),
                        )
                    })
                    .and_then(|_| stdin.write_all(header_json))
                    .and_then(|_| stdin.write_all(canonical_bgra8))
                    .map_err(|error| format!("write duel perception request: {error}"))?;
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
                    .map_err(|error| format!("poll duel perception runtime: {error}"))?
                {
                    break status;
                }
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("verified duel perception runtime timed out".to_owned());
                }
                thread::sleep(Duration::from_millis(5));
            };
            writer
                .join()
                .map_err(|_| "duel perception request writer panicked".to_owned())??;
            let (output, output_truncated) = stdout_reader
                .join()
                .map_err(|_| "duel perception stdout reader panicked".to_owned())??;
            let (stderr_bytes, stderr_truncated) = stderr_reader
                .join()
                .map_err(|_| "duel perception stderr reader panicked".to_owned())??;
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
    fn competitive_bridge_rejects_every_opaque_lineage_substitution() {
        let semantic = serde_json::json!({"kind":"pass","actor":"p0"});
        let semantic_json = serde_json::to_vec(&semantic).unwrap();
        let opaque = OpaqueDuelActionBindingViewV1 {
            opaque_control_resolution_commitment_sha256: "0".repeat(64),
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
            plan_commitment_sha256: "b".repeat(64),
        };
        validate_opaque_competitive_action_binding_v1(&opaque, &baseline, &semantic).unwrap();

        for mutation in 0..12 {
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
                _ => unreachable!(),
            }
            assert!(
                validate_opaque_competitive_action_binding_v1(&opaque, &changed, &semantic)
                    .is_err()
            );
        }
        let other_semantic = serde_json::json!({"kind":"pass","actor":"p1"});
        assert!(
            validate_opaque_competitive_action_binding_v1(&opaque, &baseline, &other_semantic)
                .is_err()
        );
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
