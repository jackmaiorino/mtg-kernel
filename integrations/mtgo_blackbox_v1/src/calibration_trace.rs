use crate::{MtgoContractErrorV1, MtgoRectPxV1, MtgoSizePxV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::rl::{ActionSemanticV1, PlayerSeatV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_PREGAME_CALIBRATION_TRACE_SCHEMA_V1: u32 = 1;
pub const MTGO_GAMEPLAY_CALIBRATION_TRACE_SCHEMA_V1: u32 = 1;
pub const MTGO_VISIBLE_OBJECT_ACTION_CALIBRATION_TRACE_SCHEMA_V1: u32 = 1;

const TRACE_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-pregame-calibration-trace-v1";
const GAMEPLAY_TRACE_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-gameplay-calibration-trace-v1";
const VISIBLE_OBJECT_ACTION_TRACE_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-visible-object-action-calibration-trace-v1";
const MAX_CLIENT_DIMENSION_V1: u32 = 16_384;

#[derive(Clone, Copy)]
struct MtgoCalibrationFrameErrorCodesV1 {
    manifest_hash_invalid: &'static str,
    pixel_hash_invalid: &'static str,
    client_size_invalid: &'static str,
    preview_claims_authority: &'static str,
}

const PREGAME_FRAME_ERROR_CODES_V1: MtgoCalibrationFrameErrorCodesV1 =
    MtgoCalibrationFrameErrorCodesV1 {
        manifest_hash_invalid: "pregame_trace_manifest_hash_invalid",
        pixel_hash_invalid: "pregame_trace_frame_hash_invalid",
        client_size_invalid: "pregame_trace_client_size_invalid",
        preview_claims_authority: "pregame_trace_preview_claims_authority",
    };

const GAMEPLAY_FRAME_ERROR_CODES_V1: MtgoCalibrationFrameErrorCodesV1 =
    MtgoCalibrationFrameErrorCodesV1 {
        manifest_hash_invalid: "gameplay_trace_manifest_hash_invalid",
        pixel_hash_invalid: "gameplay_trace_frame_hash_invalid",
        client_size_invalid: "gameplay_trace_client_size_invalid",
        preview_claims_authority: "gameplay_trace_preview_claims_authority",
    };

const VISIBLE_OBJECT_ACTION_FRAME_ERROR_CODES_V1: MtgoCalibrationFrameErrorCodesV1 =
    MtgoCalibrationFrameErrorCodesV1 {
        manifest_hash_invalid: "visible_object_action_trace_manifest_hash_invalid",
        pixel_hash_invalid: "visible_object_action_trace_frame_hash_invalid",
        client_size_invalid: "visible_object_action_trace_client_size_invalid",
        preview_claims_authority: "visible_object_action_trace_preview_claims_authority",
    };

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCalibrationPreviewStatusV1 {
    PendingVisualReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MtgoCalibrationPreviewKindV1 {
    #[serde(rename = "mtgo_visible_solitaire_gameplay_calibration_preview_v1")]
    SolitaireGameplayCalibrationPreviewV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCalibrationCaptureRoleV1 {
    ActingPlayerSolitaire,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCalibrationFrameReferenceV1 {
    pub sequence: u64,
    pub manifest_sha256: String,
    pub frame_sha256: String,
    pub client_size_px: MtgoSizePxV1,
    pub artifact_kind: MtgoCalibrationPreviewKindV1,
    pub capture_role: MtgoCalibrationCaptureRoleV1,
    pub status: MtgoCalibrationPreviewStatusV1,
    pub safe_for_semantic_evidence: bool,
    pub safe_for_ocr: bool,
    pub safe_for_policy_scoring: bool,
    pub safe_for_input: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoPregameActionSemanticV1 {
    KeepOpeningHand,
    Mulligan { next_hand_size: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoPregameVisibleChangeV1 {
    PromptChanged,
    PlayerCountsChanged,
    VisibleGameLogChanged,
    PhaseBarChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleRegionCommitmentV1 {
    pub rect_client_px: MtgoRectPxV1,
    pub bgra8_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleRegionTransitionV1 {
    pub change: MtgoPregameVisibleChangeV1,
    pub rect_client_px: MtgoRectPxV1,
    pub before_bgra8_sha256: String,
    pub after_bgra8_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPregameCalibrationTraceV1 {
    pub schema_version: u32,
    pub trace_id: String,
    pub before_frame: MtgoCalibrationFrameReferenceV1,
    pub action: MtgoPregameActionSemanticV1,
    pub action_control_before: MtgoVisibleRegionCommitmentV1,
    pub after_frame: MtgoCalibrationFrameReferenceV1,
    pub visible_postconditions: Vec<MtgoVisibleRegionTransitionV1>,
}

/// Structurally checked supervised calibration data.
///
/// This type proves only internal hash, geometry, and transition consistency. It
/// does not prove that the source pixels were captured truthfully, that a label
/// matches those pixels, or that any input is authorized. It intentionally has
/// no region, pixel, evidence, policy, or input accessor.
pub struct CheckedUntrustedMtgoPregameCalibrationV1 {
    record: MtgoPregameCalibrationTraceV1,
    transition_commitment_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoGameplayVisibleChangeV1 {
    PromptChanged,
    PhaseBarChanged,
    PlayerCountsChanged,
    BattlefieldChanged,
    HandChanged,
    VisibleGameLogChanged,
    ManaPoolChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoGameplayVisibleRegionTransitionV1 {
    pub change: MtgoGameplayVisibleChangeV1,
    pub rect_client_px: MtgoRectPxV1,
    pub before_bgra8_sha256: String,
    pub after_bgra8_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoGameplayCalibrationActionV1 {
    Pass { actor: PlayerSeatV1 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoGameplayCalibrationTraceV1 {
    pub schema_version: u32,
    pub trace_id: String,
    pub before_frame: MtgoCalibrationFrameReferenceV1,
    pub action: MtgoGameplayCalibrationActionV1,
    pub action_control_before: MtgoVisibleRegionCommitmentV1,
    pub after_frame: MtgoCalibrationFrameReferenceV1,
    pub visible_postconditions: Vec<MtgoGameplayVisibleRegionTransitionV1>,
}

/// Structurally checked calibration for one kernel-semantic action.
///
/// V1 accepts only a local-seat `Pass` transition. It is not a validated MTGO
/// decision because it does not contain a complete observation or legal-action
/// set. It has no region, pixel, evidence, policy, or input accessor.
pub struct CheckedUntrustedMtgoGameplayCalibrationV1 {
    record: MtgoGameplayCalibrationTraceV1,
    semantic: ActionSemanticV1,
    transition_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoVisibleObjectCalibrationActionV1 {
    PlayLand {
        actor: PlayerSeatV1,
        source_adapter_object_id: String,
        visible_card_name: String,
    },
    ActivateManaAbility {
        actor: PlayerSeatV1,
        source_adapter_object_id: String,
        visible_card_name: String,
        mana_choice: Option<ManaColor>,
        visible_mana_added: ManaColor,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleObjectActionCalibrationTraceV1 {
    pub schema_version: u32,
    pub trace_id: String,
    pub before_frame: MtgoCalibrationFrameReferenceV1,
    pub action: MtgoVisibleObjectCalibrationActionV1,
    pub action_control_before: MtgoVisibleRegionCommitmentV1,
    pub after_frame: MtgoCalibrationFrameReferenceV1,
    pub visible_postconditions: Vec<MtgoGameplayVisibleRegionTransitionV1>,
}

/// Structurally checked calibration for one visible object action.
///
/// V1 accepts local-seat PlayLand and ActivateManaAbility labels with an
/// adapter-local source object. It does not create the corresponding kernel
/// semantic because the trace has no complete observation or exact
/// `CardStableRefV1` binding. It has no region, pixel, evidence, policy, or
/// input accessor.
pub struct CheckedUntrustedMtgoVisibleObjectActionCalibrationV1 {
    record: MtgoVisibleObjectActionCalibrationTraceV1,
    transition_commitment_sha256: String,
}

impl CheckedUntrustedMtgoVisibleObjectActionCalibrationV1 {
    pub fn action(&self) -> &MtgoVisibleObjectCalibrationActionV1 {
        &self.record.action
    }

    pub fn before_frame_sha256(&self) -> &str {
        &self.record.before_frame.frame_sha256
    }

    pub fn after_frame_sha256(&self) -> &str {
        &self.record.after_frame.frame_sha256
    }

    pub fn transition_commitment_sha256(&self) -> &str {
        &self.transition_commitment_sha256
    }
}

impl CheckedUntrustedMtgoGameplayCalibrationV1 {
    pub fn action(&self) -> &ActionSemanticV1 {
        &self.semantic
    }

    pub fn before_frame_sha256(&self) -> &str {
        &self.record.before_frame.frame_sha256
    }

    pub fn after_frame_sha256(&self) -> &str {
        &self.record.after_frame.frame_sha256
    }

    pub fn transition_commitment_sha256(&self) -> &str {
        &self.transition_commitment_sha256
    }
}

impl CheckedUntrustedMtgoPregameCalibrationV1 {
    pub fn action(&self) -> &MtgoPregameActionSemanticV1 {
        &self.record.action
    }

    pub fn before_frame_sha256(&self) -> &str {
        &self.record.before_frame.frame_sha256
    }

    pub fn after_frame_sha256(&self) -> &str {
        &self.record.after_frame.frame_sha256
    }

    pub fn transition_commitment_sha256(&self) -> &str {
        &self.transition_commitment_sha256
    }
}

pub fn validate_pregame_calibration_trace_v1(
    record: MtgoPregameCalibrationTraceV1,
) -> Result<CheckedUntrustedMtgoPregameCalibrationV1, MtgoContractErrorV1> {
    if record.schema_version != MTGO_PREGAME_CALIBRATION_TRACE_SCHEMA_V1 {
        return Err(MtgoContractErrorV1::new(
            "pregame_trace_schema_mismatch",
            record.schema_version.to_string(),
        ));
    }
    if record.trace_id.is_empty()
        || record.trace_id.len() > 128
        || !record
            .trace_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(MtgoContractErrorV1::new(
            "pregame_trace_id_invalid",
            &record.trace_id,
        ));
    }

    validate_frame_reference_v1(&record.before_frame, PREGAME_FRAME_ERROR_CODES_V1)?;
    validate_frame_reference_v1(&record.after_frame, PREGAME_FRAME_ERROR_CODES_V1)?;
    if record.before_frame.sequence >= record.after_frame.sequence {
        return Err(MtgoContractErrorV1::new(
            "pregame_trace_frame_order_invalid",
            format!(
                "before={},after={}",
                record.before_frame.sequence, record.after_frame.sequence
            ),
        ));
    }
    if record.before_frame.client_size_px != record.after_frame.client_size_px {
        return Err(MtgoContractErrorV1::new(
            "pregame_trace_client_size_changed",
            "before and after sizes differ",
        ));
    }
    if record.before_frame.frame_sha256 == record.after_frame.frame_sha256
        || record.before_frame.manifest_sha256 == record.after_frame.manifest_sha256
    {
        return Err(MtgoContractErrorV1::new(
            "pregame_trace_source_did_not_change",
            "before and after commitments must differ",
        ));
    }

    if let MtgoPregameActionSemanticV1::Mulligan { next_hand_size } = record.action {
        if next_hand_size > 6 {
            return Err(MtgoContractErrorV1::new(
                "pregame_trace_mulligan_hand_size_invalid",
                next_hand_size.to_string(),
            ));
        }
    }

    validate_rect_v1(
        &record.action_control_before.rect_client_px,
        &record.before_frame.client_size_px,
        "pregame_trace_action_control_invalid",
    )?;
    require_sha256_v1(
        &record.action_control_before.bgra8_sha256,
        "pregame_trace_action_control_hash_invalid",
    )?;

    if !(2..=8).contains(&record.visible_postconditions.len()) {
        return Err(MtgoContractErrorV1::new(
            "pregame_trace_postcondition_count_invalid",
            record.visible_postconditions.len().to_string(),
        ));
    }
    let mut changes = HashSet::new();
    let mut rectangles = HashSet::new();
    for postcondition in &record.visible_postconditions {
        if !changes.insert(postcondition.change) {
            return Err(MtgoContractErrorV1::new(
                "pregame_trace_duplicate_visible_change",
                format!("{:?}", postcondition.change),
            ));
        }
        let rect = &postcondition.rect_client_px;
        validate_rect_v1(
            rect,
            &record.before_frame.client_size_px,
            "pregame_trace_postcondition_rect_invalid",
        )?;
        if !rectangles.insert((rect.x, rect.y, rect.width, rect.height)) {
            return Err(MtgoContractErrorV1::new(
                "pregame_trace_duplicate_postcondition_rect",
                format!("{},{},{},{}", rect.x, rect.y, rect.width, rect.height),
            ));
        }
        require_sha256_v1(
            &postcondition.before_bgra8_sha256,
            "pregame_trace_postcondition_hash_invalid",
        )?;
        require_sha256_v1(
            &postcondition.after_bgra8_sha256,
            "pregame_trace_postcondition_hash_invalid",
        )?;
        if postcondition.before_bgra8_sha256 == postcondition.after_bgra8_sha256 {
            return Err(MtgoContractErrorV1::new(
                "pregame_trace_postcondition_unchanged",
                format!("{:?}", postcondition.change),
            ));
        }
    }

    let required_changes: &[MtgoPregameVisibleChangeV1] = match record.action {
        MtgoPregameActionSemanticV1::KeepOpeningHand => &[
            MtgoPregameVisibleChangeV1::PromptChanged,
            MtgoPregameVisibleChangeV1::PlayerCountsChanged,
            MtgoPregameVisibleChangeV1::VisibleGameLogChanged,
            MtgoPregameVisibleChangeV1::PhaseBarChanged,
        ],
        MtgoPregameActionSemanticV1::Mulligan { .. } => &[
            MtgoPregameVisibleChangeV1::PromptChanged,
            MtgoPregameVisibleChangeV1::PlayerCountsChanged,
            MtgoPregameVisibleChangeV1::VisibleGameLogChanged,
        ],
    };
    for required in required_changes {
        if !changes.contains(required) {
            return Err(MtgoContractErrorV1::new(
                "pregame_trace_required_postcondition_missing",
                format!("{:?}", required),
            ));
        }
    }

    let encoded = serde_json::to_vec(&record).map_err(|error| {
        MtgoContractErrorV1::new("pregame_trace_serialization_failed", error.to_string())
    })?;
    let mut hasher = Sha256::new();
    hasher.update(TRACE_COMMITMENT_DOMAIN_V1);
    hasher.update((encoded.len() as u64).to_le_bytes());
    hasher.update(&encoded);
    let transition_commitment_sha256 = format!("{:x}", hasher.finalize());

    Ok(CheckedUntrustedMtgoPregameCalibrationV1 {
        record,
        transition_commitment_sha256,
    })
}

pub fn validate_gameplay_calibration_trace_v1(
    record: MtgoGameplayCalibrationTraceV1,
) -> Result<CheckedUntrustedMtgoGameplayCalibrationV1, MtgoContractErrorV1> {
    if record.schema_version != MTGO_GAMEPLAY_CALIBRATION_TRACE_SCHEMA_V1 {
        return Err(MtgoContractErrorV1::new(
            "gameplay_trace_schema_mismatch",
            record.schema_version.to_string(),
        ));
    }
    validate_trace_id_v1(&record.trace_id, "gameplay_trace_id_invalid")?;
    validate_gameplay_frame_pair_v1(&record.before_frame, &record.after_frame)?;

    let semantic = match &record.action {
        MtgoGameplayCalibrationActionV1::Pass {
            actor: PlayerSeatV1::P0,
        } => ActionSemanticV1::Pass {
            actor: PlayerSeatV1::P0,
        },
        _ => {
            return Err(MtgoContractErrorV1::new(
                "gameplay_trace_action_unsupported",
                "v1 accepts only Pass by the local P0 seat",
            ));
        }
    };

    validate_rect_v1(
        &record.action_control_before.rect_client_px,
        &record.before_frame.client_size_px,
        "gameplay_trace_action_control_invalid",
    )?;
    require_sha256_v1(
        &record.action_control_before.bgra8_sha256,
        "gameplay_trace_action_control_hash_invalid",
    )?;
    if !(2..=8).contains(&record.visible_postconditions.len()) {
        return Err(MtgoContractErrorV1::new(
            "gameplay_trace_postcondition_count_invalid",
            record.visible_postconditions.len().to_string(),
        ));
    }
    let mut changes = HashSet::new();
    let mut rectangles = HashSet::new();
    for postcondition in &record.visible_postconditions {
        if !changes.insert(postcondition.change) {
            return Err(MtgoContractErrorV1::new(
                "gameplay_trace_duplicate_visible_change",
                format!("{:?}", postcondition.change),
            ));
        }
        let rect = &postcondition.rect_client_px;
        validate_rect_v1(
            rect,
            &record.before_frame.client_size_px,
            "gameplay_trace_postcondition_rect_invalid",
        )?;
        if !rectangles.insert((rect.x, rect.y, rect.width, rect.height)) {
            return Err(MtgoContractErrorV1::new(
                "gameplay_trace_duplicate_postcondition_rect",
                format!("{},{},{},{}", rect.x, rect.y, rect.width, rect.height),
            ));
        }
        require_sha256_v1(
            &postcondition.before_bgra8_sha256,
            "gameplay_trace_postcondition_hash_invalid",
        )?;
        require_sha256_v1(
            &postcondition.after_bgra8_sha256,
            "gameplay_trace_postcondition_hash_invalid",
        )?;
        if postcondition.before_bgra8_sha256 == postcondition.after_bgra8_sha256 {
            return Err(MtgoContractErrorV1::new(
                "gameplay_trace_postcondition_unchanged",
                format!("{:?}", postcondition.change),
            ));
        }
    }
    for required in [
        MtgoGameplayVisibleChangeV1::PromptChanged,
        MtgoGameplayVisibleChangeV1::PhaseBarChanged,
    ] {
        if !changes.contains(&required) {
            return Err(MtgoContractErrorV1::new(
                "gameplay_trace_required_postcondition_missing",
                format!("{:?}", required),
            ));
        }
    }

    let encoded = serde_json::to_vec(&record).map_err(|error| {
        MtgoContractErrorV1::new("gameplay_trace_serialization_failed", error.to_string())
    })?;
    let transition_commitment_sha256 =
        transition_commitment_v1(GAMEPLAY_TRACE_COMMITMENT_DOMAIN_V1, &encoded);
    Ok(CheckedUntrustedMtgoGameplayCalibrationV1 {
        record,
        semantic,
        transition_commitment_sha256,
    })
}

pub fn validate_visible_object_action_calibration_trace_v1(
    record: MtgoVisibleObjectActionCalibrationTraceV1,
) -> Result<CheckedUntrustedMtgoVisibleObjectActionCalibrationV1, MtgoContractErrorV1> {
    if record.schema_version != MTGO_VISIBLE_OBJECT_ACTION_CALIBRATION_TRACE_SCHEMA_V1 {
        return Err(MtgoContractErrorV1::new(
            "visible_object_action_trace_schema_mismatch",
            record.schema_version.to_string(),
        ));
    }
    validate_trace_id_v1(&record.trace_id, "visible_object_action_trace_id_invalid")?;
    validate_visible_object_action_frame_pair_v1(&record.before_frame, &record.after_frame)?;

    let required_changes: &[MtgoGameplayVisibleChangeV1] = match &record.action {
        MtgoVisibleObjectCalibrationActionV1::PlayLand {
            actor: PlayerSeatV1::P0,
            source_adapter_object_id,
            visible_card_name,
        } => {
            validate_adapter_object_id_v1(source_adapter_object_id)?;
            validate_visible_card_name_v1(visible_card_name)?;
            &[
                MtgoGameplayVisibleChangeV1::PromptChanged,
                MtgoGameplayVisibleChangeV1::PlayerCountsChanged,
                MtgoGameplayVisibleChangeV1::BattlefieldChanged,
                MtgoGameplayVisibleChangeV1::HandChanged,
                MtgoGameplayVisibleChangeV1::VisibleGameLogChanged,
            ]
        }
        MtgoVisibleObjectCalibrationActionV1::ActivateManaAbility {
            actor: PlayerSeatV1::P0,
            source_adapter_object_id,
            visible_card_name,
            mana_choice,
            visible_mana_added,
        } => {
            validate_adapter_object_id_v1(source_adapter_object_id)?;
            validate_visible_card_name_v1(visible_card_name)?;
            if mana_choice.is_some_and(|choice| choice != *visible_mana_added) {
                return Err(MtgoContractErrorV1::new(
                    "visible_object_action_trace_mana_choice_mismatch",
                    "an explicit mana choice must match the visibly added mana",
                ));
            }
            &[
                MtgoGameplayVisibleChangeV1::BattlefieldChanged,
                MtgoGameplayVisibleChangeV1::ManaPoolChanged,
            ]
        }
        _ => {
            return Err(MtgoContractErrorV1::new(
                "visible_object_action_trace_action_unsupported",
                "v1 accepts only supported visible-object actions by the local P0 seat",
            ));
        }
    };

    validate_rect_v1(
        &record.action_control_before.rect_client_px,
        &record.before_frame.client_size_px,
        "visible_object_action_trace_action_control_invalid",
    )?;
    require_sha256_v1(
        &record.action_control_before.bgra8_sha256,
        "visible_object_action_trace_action_control_hash_invalid",
    )?;

    if !(required_changes.len()..=8).contains(&record.visible_postconditions.len()) {
        return Err(MtgoContractErrorV1::new(
            "visible_object_action_trace_postcondition_count_invalid",
            record.visible_postconditions.len().to_string(),
        ));
    }
    let mut changes = HashSet::new();
    let mut rectangles = HashSet::new();
    for postcondition in &record.visible_postconditions {
        if !changes.insert(postcondition.change) {
            return Err(MtgoContractErrorV1::new(
                "visible_object_action_trace_duplicate_visible_change",
                format!("{:?}", postcondition.change),
            ));
        }
        let rect = &postcondition.rect_client_px;
        validate_rect_v1(
            rect,
            &record.before_frame.client_size_px,
            "visible_object_action_trace_postcondition_rect_invalid",
        )?;
        if !rectangles.insert((rect.x, rect.y, rect.width, rect.height)) {
            return Err(MtgoContractErrorV1::new(
                "visible_object_action_trace_duplicate_postcondition_rect",
                format!("{},{},{},{}", rect.x, rect.y, rect.width, rect.height),
            ));
        }
        require_sha256_v1(
            &postcondition.before_bgra8_sha256,
            "visible_object_action_trace_postcondition_hash_invalid",
        )?;
        require_sha256_v1(
            &postcondition.after_bgra8_sha256,
            "visible_object_action_trace_postcondition_hash_invalid",
        )?;
        if postcondition.before_bgra8_sha256 == postcondition.after_bgra8_sha256 {
            return Err(MtgoContractErrorV1::new(
                "visible_object_action_trace_postcondition_unchanged",
                format!("{:?}", postcondition.change),
            ));
        }
    }
    for required in required_changes {
        if !changes.contains(required) {
            return Err(MtgoContractErrorV1::new(
                "visible_object_action_trace_required_postcondition_missing",
                format!("{:?}", required),
            ));
        }
    }

    let encoded = serde_json::to_vec(&record).map_err(|error| {
        MtgoContractErrorV1::new(
            "visible_object_action_trace_serialization_failed",
            error.to_string(),
        )
    })?;
    let transition_commitment_sha256 =
        transition_commitment_v1(VISIBLE_OBJECT_ACTION_TRACE_COMMITMENT_DOMAIN_V1, &encoded);
    Ok(CheckedUntrustedMtgoVisibleObjectActionCalibrationV1 {
        record,
        transition_commitment_sha256,
    })
}

fn validate_frame_reference_v1(
    frame: &MtgoCalibrationFrameReferenceV1,
    error_codes: MtgoCalibrationFrameErrorCodesV1,
) -> Result<(), MtgoContractErrorV1> {
    require_sha256_v1(&frame.manifest_sha256, error_codes.manifest_hash_invalid)?;
    require_sha256_v1(&frame.frame_sha256, error_codes.pixel_hash_invalid)?;
    if frame.client_size_px.width == 0
        || frame.client_size_px.height == 0
        || frame.client_size_px.width > MAX_CLIENT_DIMENSION_V1
        || frame.client_size_px.height > MAX_CLIENT_DIMENSION_V1
    {
        return Err(MtgoContractErrorV1::new(
            error_codes.client_size_invalid,
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
            error_codes.preview_claims_authority,
            "calibration preview safety flags must all remain false",
        ));
    }
    Ok(())
}

fn validate_gameplay_frame_pair_v1(
    before: &MtgoCalibrationFrameReferenceV1,
    after: &MtgoCalibrationFrameReferenceV1,
) -> Result<(), MtgoContractErrorV1> {
    validate_frame_reference_v1(before, GAMEPLAY_FRAME_ERROR_CODES_V1)?;
    validate_frame_reference_v1(after, GAMEPLAY_FRAME_ERROR_CODES_V1)?;
    if before.sequence >= after.sequence {
        return Err(MtgoContractErrorV1::new(
            "gameplay_trace_frame_order_invalid",
            format!("before={},after={}", before.sequence, after.sequence),
        ));
    }
    if before.client_size_px != after.client_size_px {
        return Err(MtgoContractErrorV1::new(
            "gameplay_trace_client_size_changed",
            "before and after sizes differ",
        ));
    }
    if before.frame_sha256 == after.frame_sha256 || before.manifest_sha256 == after.manifest_sha256
    {
        return Err(MtgoContractErrorV1::new(
            "gameplay_trace_source_did_not_change",
            "before and after commitments must differ",
        ));
    }
    Ok(())
}

fn validate_visible_object_action_frame_pair_v1(
    before: &MtgoCalibrationFrameReferenceV1,
    after: &MtgoCalibrationFrameReferenceV1,
) -> Result<(), MtgoContractErrorV1> {
    validate_frame_reference_v1(before, VISIBLE_OBJECT_ACTION_FRAME_ERROR_CODES_V1)?;
    validate_frame_reference_v1(after, VISIBLE_OBJECT_ACTION_FRAME_ERROR_CODES_V1)?;
    if before.sequence >= after.sequence {
        return Err(MtgoContractErrorV1::new(
            "visible_object_action_trace_frame_order_invalid",
            format!("before={},after={}", before.sequence, after.sequence),
        ));
    }
    if before.client_size_px != after.client_size_px {
        return Err(MtgoContractErrorV1::new(
            "visible_object_action_trace_client_size_changed",
            "before and after sizes differ",
        ));
    }
    if before.frame_sha256 == after.frame_sha256 || before.manifest_sha256 == after.manifest_sha256
    {
        return Err(MtgoContractErrorV1::new(
            "visible_object_action_trace_source_did_not_change",
            "before and after commitments must differ",
        ));
    }
    Ok(())
}

fn validate_adapter_object_id_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(MtgoContractErrorV1::new(
            "visible_object_action_trace_source_object_id_invalid",
            value,
        ));
    }
    Ok(())
}

fn validate_visible_card_name_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(MtgoContractErrorV1::new(
            "visible_object_action_trace_card_name_invalid",
            value,
        ));
    }
    Ok(())
}

fn validate_trace_id_v1(trace_id: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if trace_id.is_empty()
        || trace_id.len() > 128
        || !trace_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(MtgoContractErrorV1::new(code, trace_id));
    }
    Ok(())
}

fn transition_commitment_v1(domain: &[u8], encoded: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update((encoded.len() as u64).to_le_bytes());
    hasher.update(encoded);
    format!("{:x}", hasher.finalize())
}

fn validate_rect_v1(
    rect: &MtgoRectPxV1,
    size: &MtgoSizePxV1,
    code: &'static str,
) -> Result<(), MtgoContractErrorV1> {
    let right = rect.x.checked_add(rect.width);
    let bottom = rect.y.checked_add(rect.height);
    if rect.width == 0
        || rect.height == 0
        || right.is_none()
        || bottom.is_none()
        || right.unwrap() > size.width
        || bottom.unwrap() > size.height
    {
        return Err(MtgoContractErrorV1::new(
            code,
            format!("{},{},{},{}", rect.x, rect.y, rect.width, rect.height),
        ));
    }
    Ok(())
}

fn require_sha256_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(MtgoContractErrorV1::new(code, value));
    }
    Ok(())
}
