use crate::{
    MtgoCalibrationCaptureRoleV1, MtgoCalibrationFrameReferenceV1, MtgoCalibrationPreviewKindV1,
    MtgoContractErrorV1, MtgoRectPxV1, MtgoVisibleRegionCommitmentV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_OBSERVATION_RECONSTRUCTION_AUDIT_SCHEMA_V1: u32 = 1;

const RECONSTRUCTION_AUDIT_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-observation-reconstruction-audit-v1";
const MAX_CLIENT_DIMENSION_V1: u32 = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoReconstructionTopologyV1 {
    TwoPlayerDuel,
    SolitaireCalibration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoObservationReconstructionGroupV1 {
    DuelParticipants,
    TurnPhaseAndPriority,
    PlayerPublicState,
    PublicObjectsAndZones,
    ActingPlayerPrivateKnowledge,
    StackCombatAndPendingChoices,
    KernelDecisionHistoryContext,
    ObjectIncarnationsAndCardDb,
    CompleteOrderedLegalActions,
    KernelContractMetadata,
}

const REQUIRED_GROUPS_V1: [MtgoObservationReconstructionGroupV1; 10] = [
    MtgoObservationReconstructionGroupV1::DuelParticipants,
    MtgoObservationReconstructionGroupV1::TurnPhaseAndPriority,
    MtgoObservationReconstructionGroupV1::PlayerPublicState,
    MtgoObservationReconstructionGroupV1::PublicObjectsAndZones,
    MtgoObservationReconstructionGroupV1::ActingPlayerPrivateKnowledge,
    MtgoObservationReconstructionGroupV1::StackCombatAndPendingChoices,
    MtgoObservationReconstructionGroupV1::KernelDecisionHistoryContext,
    MtgoObservationReconstructionGroupV1::ObjectIncarnationsAndCardDb,
    MtgoObservationReconstructionGroupV1::CompleteOrderedLegalActions,
    MtgoObservationReconstructionGroupV1::KernelContractMetadata,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoReconstructionStatusV1 {
    VisibleComplete,
    LocalDerivedComplete,
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoObservationReconstructionGroupAuditV1 {
    pub group: MtgoObservationReconstructionGroupV1,
    pub status: MtgoReconstructionStatusV1,
    pub visible_regions: Vec<MtgoVisibleRegionCommitmentV1>,
    pub missing_reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoObservationReconstructionAuditV1 {
    pub schema_version: u32,
    pub audit_id: String,
    pub topology: MtgoReconstructionTopologyV1,
    pub frame: MtgoCalibrationFrameReferenceV1,
    pub groups: Vec<MtgoObservationReconstructionGroupAuditV1>,
    pub observation_complete: bool,
    pub legal_action_set_complete: bool,
    pub ready_for_model_scoring: bool,
}

/// Structurally checked reconstruction-readiness audit.
///
/// This wrapper exposes only readiness, blockers, topology, source-frame
/// commitment, and the audit commitment. It cannot expose pixels, regions,
/// an observation, legal actions, policy scores, or input authority.
///
/// ```compile_fail
/// use mtg_kernel::rl::ObservationV5;
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoObservationReconstructionAuditV1;
/// fn cannot_extract_observation(
///     audit: &CheckedUntrustedMtgoObservationReconstructionAuditV1,
/// ) -> &ObservationV5 {
///     audit.observation()
/// }
/// ```
pub struct CheckedUntrustedMtgoObservationReconstructionAuditV1 {
    record: MtgoObservationReconstructionAuditV1,
    blocking_groups: Vec<MtgoObservationReconstructionGroupV1>,
    audit_commitment_sha256: String,
}

impl CheckedUntrustedMtgoObservationReconstructionAuditV1 {
    pub fn topology(&self) -> MtgoReconstructionTopologyV1 {
        self.record.topology
    }

    pub fn source_frame_sha256(&self) -> &str {
        &self.record.frame.frame_sha256
    }

    pub fn source_manifest_sha256(&self) -> &str {
        &self.record.frame.manifest_sha256
    }

    pub fn capture_role(&self) -> MtgoCalibrationCaptureRoleV1 {
        self.record.frame.capture_role
    }

    pub fn blocking_groups(&self) -> &[MtgoObservationReconstructionGroupV1] {
        &self.blocking_groups
    }

    pub fn observation_complete(&self) -> bool {
        self.record.observation_complete
    }

    pub fn legal_action_set_complete(&self) -> bool {
        self.record.legal_action_set_complete
    }

    pub fn ready_for_model_scoring(&self) -> bool {
        self.record.ready_for_model_scoring
    }

    pub fn audit_commitment_sha256(&self) -> &str {
        &self.audit_commitment_sha256
    }
}

pub fn validate_observation_reconstruction_audit_v1(
    record: MtgoObservationReconstructionAuditV1,
) -> Result<CheckedUntrustedMtgoObservationReconstructionAuditV1, MtgoContractErrorV1> {
    if record.schema_version != MTGO_OBSERVATION_RECONSTRUCTION_AUDIT_SCHEMA_V1 {
        return Err(MtgoContractErrorV1::new(
            "reconstruction_audit_schema_mismatch",
            record.schema_version.to_string(),
        ));
    }
    validate_safe_identifier_v1(&record.audit_id, "reconstruction_audit_id_invalid", 128)?;
    validate_frame_v1(&record.frame)?;
    validate_topology_frame_role_v1(record.topology, &record.frame)?;

    if record.groups.len() != REQUIRED_GROUPS_V1.len() {
        return Err(MtgoContractErrorV1::new(
            "reconstruction_audit_group_count_invalid",
            record.groups.len().to_string(),
        ));
    }
    let mut seen = HashSet::new();
    for (index, group_audit) in record.groups.iter().enumerate() {
        if !seen.insert(group_audit.group) {
            return Err(MtgoContractErrorV1::new(
                "reconstruction_audit_duplicate_group",
                format!("{:?}", group_audit.group),
            ));
        }
        if group_audit.group != REQUIRED_GROUPS_V1[index] {
            return Err(MtgoContractErrorV1::new(
                "reconstruction_audit_group_order_invalid",
                format!("index={index},group={:?}", group_audit.group),
            ));
        }
        validate_group_audit_v1(group_audit, &record.frame)?;
    }

    let legal_group = record
        .groups
        .iter()
        .find(|group| {
            group.group == MtgoObservationReconstructionGroupV1::CompleteOrderedLegalActions
        })
        .expect("required canonical group exists");
    if record.frame.capture_role == MtgoCalibrationCaptureRoleV1::Spectator {
        validate_spectator_limit_v1(
            &record,
            MtgoObservationReconstructionGroupV1::TurnPhaseAndPriority,
            "spectator_priority_not_acting_player_equivalent",
        )?;
        validate_spectator_limit_v1(
            &record,
            MtgoObservationReconstructionGroupV1::ActingPlayerPrivateKnowledge,
            "spectator_role_cannot_supply_acting_private_knowledge",
        )?;
        validate_spectator_limit_v1(
            &record,
            MtgoObservationReconstructionGroupV1::CompleteOrderedLegalActions,
            "spectator_role_cannot_supply_acting_legal_actions",
        )?;
    }
    let legal_action_set_complete =
        legal_group.status == MtgoReconstructionStatusV1::VisibleComplete;
    let observation_complete = record.topology == MtgoReconstructionTopologyV1::TwoPlayerDuel
        && record
            .groups
            .iter()
            .filter(|group| {
                group.group != MtgoObservationReconstructionGroupV1::CompleteOrderedLegalActions
            })
            .all(|group| group.status != MtgoReconstructionStatusV1::Incomplete);
    if record.ready_for_model_scoring {
        return Err(MtgoContractErrorV1::new(
            "reconstruction_audit_model_scoring_authority_forbidden",
            "an untrusted reconstruction audit can never grant model-scoring authority",
        ));
    }

    if record.observation_complete != observation_complete
        || record.legal_action_set_complete != legal_action_set_complete
    {
        return Err(MtgoContractErrorV1::new(
            "reconstruction_audit_readiness_mismatch",
            format!("observation={observation_complete},actions={legal_action_set_complete}"),
        ));
    }

    let blocking_groups = record
        .groups
        .iter()
        .filter(|group| group.status == MtgoReconstructionStatusV1::Incomplete)
        .map(|group| group.group)
        .collect();
    let encoded = serde_json::to_vec(&record).map_err(|error| {
        MtgoContractErrorV1::new(
            "reconstruction_audit_serialization_failed",
            error.to_string(),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(RECONSTRUCTION_AUDIT_COMMITMENT_DOMAIN_V1);
    hasher.update((encoded.len() as u64).to_le_bytes());
    hasher.update(&encoded);
    let audit_commitment_sha256 = format!("{:x}", hasher.finalize());

    Ok(CheckedUntrustedMtgoObservationReconstructionAuditV1 {
        record,
        blocking_groups,
        audit_commitment_sha256,
    })
}

fn validate_topology_frame_role_v1(
    topology: MtgoReconstructionTopologyV1,
    frame: &MtgoCalibrationFrameReferenceV1,
) -> Result<(), MtgoContractErrorV1> {
    let compatible = matches!(
        (topology, frame.artifact_kind, frame.capture_role),
        (
            MtgoReconstructionTopologyV1::SolitaireCalibration,
            MtgoCalibrationPreviewKindV1::SolitaireGameplayCalibrationPreviewV1,
            MtgoCalibrationCaptureRoleV1::ActingPlayerSolitaire,
        ) | (
            MtgoReconstructionTopologyV1::TwoPlayerDuel,
            MtgoCalibrationPreviewKindV1::SpectatorGameplayCalibrationPreviewV1,
            MtgoCalibrationCaptureRoleV1::Spectator,
        ) | (
            MtgoReconstructionTopologyV1::TwoPlayerDuel,
            MtgoCalibrationPreviewKindV1::ActingPlayerDuelGameplayCalibrationPreviewV1,
            MtgoCalibrationCaptureRoleV1::ActingPlayerDuel,
        )
    );
    if !compatible {
        return Err(MtgoContractErrorV1::new(
            "reconstruction_audit_frame_role_mismatch",
            format!(
                "topology={topology:?},artifact={:?},role={:?}",
                frame.artifact_kind, frame.capture_role
            ),
        ));
    }
    Ok(())
}

fn validate_spectator_limit_v1(
    record: &MtgoObservationReconstructionAuditV1,
    group: MtgoObservationReconstructionGroupV1,
    required_reason: &'static str,
) -> Result<(), MtgoContractErrorV1> {
    let group_audit = record
        .groups
        .iter()
        .find(|candidate| candidate.group == group)
        .expect("required canonical group exists");
    if group_audit.status != MtgoReconstructionStatusV1::Incomplete
        || !group_audit
            .missing_reason_codes
            .iter()
            .any(|reason| reason == required_reason)
    {
        return Err(MtgoContractErrorV1::new(
            "reconstruction_audit_spectator_authority_forbidden",
            format!("group={group:?},required_reason={required_reason}"),
        ));
    }
    Ok(())
}

fn validate_group_audit_v1(
    group: &MtgoObservationReconstructionGroupAuditV1,
    frame: &MtgoCalibrationFrameReferenceV1,
) -> Result<(), MtgoContractErrorV1> {
    if group.visible_regions.len() > 8 || group.missing_reason_codes.len() > 16 {
        return Err(MtgoContractErrorV1::new(
            "reconstruction_audit_group_detail_count_invalid",
            format!("{:?}", group.group),
        ));
    }
    for region in &group.visible_regions {
        validate_rect_v1(&region.rect_client_px, frame)?;
        validate_sha256_v1(
            &region.bgra8_sha256,
            "reconstruction_audit_region_hash_invalid",
        )?;
    }
    let mut reasons = HashSet::new();
    for reason in &group.missing_reason_codes {
        validate_safe_identifier_v1(reason, "reconstruction_audit_reason_code_invalid", 128)?;
        if !reasons.insert(reason.as_str()) {
            return Err(MtgoContractErrorV1::new(
                "reconstruction_audit_duplicate_reason_code",
                reason,
            ));
        }
    }

    match group.status {
        MtgoReconstructionStatusV1::VisibleComplete => {
            if matches!(
                group.group,
                MtgoObservationReconstructionGroupV1::KernelDecisionHistoryContext
                    | MtgoObservationReconstructionGroupV1::ObjectIncarnationsAndCardDb
                    | MtgoObservationReconstructionGroupV1::KernelContractMetadata
            ) || group.visible_regions.is_empty()
                || !group.missing_reason_codes.is_empty()
            {
                return Err(MtgoContractErrorV1::new(
                    "reconstruction_audit_visible_complete_invalid",
                    format!("{:?}", group.group),
                ));
            }
        }
        MtgoReconstructionStatusV1::LocalDerivedComplete => {
            if !matches!(
                group.group,
                MtgoObservationReconstructionGroupV1::KernelDecisionHistoryContext
                    | MtgoObservationReconstructionGroupV1::ObjectIncarnationsAndCardDb
                    | MtgoObservationReconstructionGroupV1::KernelContractMetadata
            ) || !group.visible_regions.is_empty()
                || !group.missing_reason_codes.is_empty()
            {
                return Err(MtgoContractErrorV1::new(
                    "reconstruction_audit_local_complete_invalid",
                    format!("{:?}", group.group),
                ));
            }
        }
        MtgoReconstructionStatusV1::Incomplete => {
            if group.missing_reason_codes.is_empty() {
                return Err(MtgoContractErrorV1::new(
                    "reconstruction_audit_incomplete_without_reason",
                    format!("{:?}", group.group),
                ));
            }
        }
    }
    Ok(())
}

fn validate_frame_v1(frame: &MtgoCalibrationFrameReferenceV1) -> Result<(), MtgoContractErrorV1> {
    validate_sha256_v1(
        &frame.manifest_sha256,
        "reconstruction_audit_manifest_hash_invalid",
    )?;
    validate_sha256_v1(
        &frame.frame_sha256,
        "reconstruction_audit_frame_hash_invalid",
    )?;
    if frame.client_size_px.width == 0
        || frame.client_size_px.height == 0
        || frame.client_size_px.width > MAX_CLIENT_DIMENSION_V1
        || frame.client_size_px.height > MAX_CLIENT_DIMENSION_V1
    {
        return Err(MtgoContractErrorV1::new(
            "reconstruction_audit_client_size_invalid",
            format!(
                "{}x{}",
                frame.client_size_px.width, frame.client_size_px.height
            ),
        ));
    }
    if frame.safe_for_semantic_evidence
        || frame.safe_for_ocr
        || frame.safe_for_policy_scoring
        || frame.safe_for_input
    {
        return Err(MtgoContractErrorV1::new(
            "reconstruction_audit_preview_claims_authority",
            "calibration preview safety flags must all remain false",
        ));
    }
    Ok(())
}

fn validate_rect_v1(
    rect: &MtgoRectPxV1,
    frame: &MtgoCalibrationFrameReferenceV1,
) -> Result<(), MtgoContractErrorV1> {
    let right = rect.x.checked_add(rect.width);
    let bottom = rect.y.checked_add(rect.height);
    if rect.width == 0
        || rect.height == 0
        || right.is_none_or(|value| value > frame.client_size_px.width)
        || bottom.is_none_or(|value| value > frame.client_size_px.height)
    {
        return Err(MtgoContractErrorV1::new(
            "reconstruction_audit_region_rect_invalid",
            format!("{},{},{},{}", rect.x, rect.y, rect.width, rect.height),
        ));
    }
    Ok(())
}

fn validate_sha256_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(MtgoContractErrorV1::new(code, value));
    }
    Ok(())
}

fn validate_safe_identifier_v1(
    value: &str,
    code: &'static str,
    max_len: usize,
) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > max_len
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(MtgoContractErrorV1::new(code, value));
    }
    Ok(())
}
