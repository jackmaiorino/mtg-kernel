use crate::{
    CheckedUntrustedMtgoModelSelectionV1, MtgoContractErrorV1, MtgoEvidenceSourceV1, MtgoRectPxV1,
    ValidatedMtgoObservedDecisionV1, MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1,
};
use mtg_kernel::rl::ActionSemanticV1;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_VISIBLE_ACTION_CONTROL_SET_SCHEMA_V1: u32 = 1;

const RESOLUTION_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-visible-action-resolution-v1";
const MAX_VISIBLE_CONTROLS_V1: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoVisibleControlKindV1 {
    PromptButton,
    PhaseButton,
    Card,
    AbilityMenuItem,
    ChoiceItem,
    ConfirmationButton,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleActionControlCandidateV1 {
    pub control_id: String,
    pub control_kind: MtgoVisibleControlKindV1,
    pub frame_region_evidence_id: u64,
    pub semantic: ActionSemanticV1,
    pub confidence_bps: u16,
    pub visibly_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleActionControlSetV1 {
    pub schema_version: u32,
    pub decision_commitment_sha256: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub prompt_frame_region_evidence_id: u64,
    pub prompt_reconciled: bool,
    pub candidate_set_complete: bool,
    pub controls: Vec<MtgoVisibleActionControlCandidateV1>,
}

/// One unique current-frame visible control resolved from the selected semantic.
///
/// The rectangle remains private. This structural result does not prove that a
/// perception label matches its pixels and cannot authorize or perform input.
/// A future trusted live actuator must revalidate the current frame, window,
/// prompt, timer, and point ownership immediately before one input.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoResolvedActionControlV1;
/// fn cannot_extract_coordinates(control: &CheckedUntrustedMtgoResolvedActionControlV1) {
///     let _ = control.rect_client_px();
/// }
/// ```
pub struct CheckedUntrustedMtgoResolvedActionControlV1 {
    decision_commitment_sha256: String,
    selection_commitment_sha256: String,
    control_id: String,
    frame_id: u64,
    frame_sequence: u64,
    frame_region_evidence_id: u64,
    rect_client_px: MtgoRectPxV1,
    resolution_commitment_sha256: String,
}

impl CheckedUntrustedMtgoResolvedActionControlV1 {
    pub fn decision_commitment_sha256(&self) -> &str {
        &self.decision_commitment_sha256
    }

    pub fn selection_commitment_sha256(&self) -> &str {
        &self.selection_commitment_sha256
    }

    pub fn control_id(&self) -> &str {
        &self.control_id
    }

    pub fn frame_id(&self) -> u64 {
        self.frame_id
    }

    pub fn frame_sequence(&self) -> u64 {
        self.frame_sequence
    }

    pub fn frame_region_evidence_id(&self) -> u64 {
        self.frame_region_evidence_id
    }

    pub fn resolution_commitment_sha256(&self) -> &str {
        &self.resolution_commitment_sha256
    }

    pub fn safe_for_live_input(&self) -> bool {
        false
    }

    #[allow(dead_code)]
    pub(crate) fn rect_client_px(&self) -> &MtgoRectPxV1 {
        &self.rect_client_px
    }
}

pub fn resolve_selected_visible_control_v1(
    decision: &ValidatedMtgoObservedDecisionV1,
    selection: &CheckedUntrustedMtgoModelSelectionV1,
    control_set: MtgoVisibleActionControlSetV1,
) -> Result<CheckedUntrustedMtgoResolvedActionControlV1, MtgoContractErrorV1> {
    if control_set.schema_version != MTGO_VISIBLE_ACTION_CONTROL_SET_SCHEMA_V1 {
        return Err(MtgoContractErrorV1::new(
            "visible_control_set_schema_mismatch",
            control_set.schema_version.to_string(),
        ));
    }
    require_sha256_v1(
        &control_set.decision_commitment_sha256,
        "visible_control_set_decision_hash_invalid",
    )?;
    if control_set.decision_commitment_sha256 != decision.decision_commitment_sha256()
        || selection.decision_commitment_sha256() != decision.decision_commitment_sha256()
    {
        return Err(MtgoContractErrorV1::new(
            "visible_control_set_decision_mismatch",
            "control set and selection must bind the exact validated decision",
        ));
    }
    if control_set.frame_id != decision.frame_id()
        || control_set.frame_sequence != decision.frame_sequence()
    {
        return Err(MtgoContractErrorV1::new(
            "visible_control_set_stale_frame",
            "control set must bind the current validated decision frame",
        ));
    }
    if !control_set.prompt_reconciled || !control_set.candidate_set_complete {
        return Err(MtgoContractErrorV1::new(
            "visible_control_set_incomplete",
            "prompt and complete visible candidate set must be reconciled",
        ));
    }
    let prompt_region = exact_current_frame_region_v1(
        decision,
        control_set.prompt_frame_region_evidence_id,
        "visible_control_set_prompt_evidence_invalid",
    )?;
    if control_set.controls.is_empty() || control_set.controls.len() > MAX_VISIBLE_CONTROLS_V1 {
        return Err(MtgoContractErrorV1::new(
            "visible_control_count_invalid",
            control_set.controls.len().to_string(),
        ));
    }

    let mut control_ids = HashSet::new();
    let mut evidence_ids = HashSet::new();
    let mut selected_matches = Vec::new();
    for control in &control_set.controls {
        validate_safe_identifier_v1(&control.control_id, "visible_control_id_invalid")?;
        if !control_ids.insert(control.control_id.as_str()) {
            return Err(MtgoContractErrorV1::new(
                "visible_control_id_duplicate",
                &control.control_id,
            ));
        }
        if !evidence_ids.insert(control.frame_region_evidence_id) {
            return Err(MtgoContractErrorV1::new(
                "visible_control_evidence_duplicate",
                control.frame_region_evidence_id.to_string(),
            ));
        }
        if !control.visibly_enabled {
            return Err(MtgoContractErrorV1::new(
                "visible_control_disabled",
                &control.control_id,
            ));
        }
        if control.confidence_bps < MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1
            || control.confidence_bps > 10_000
        {
            return Err(MtgoContractErrorV1::new(
                "visible_control_confidence_invalid",
                format!("{}={}", control.control_id, control.confidence_bps),
            ));
        }
        if !decision.legal_actions().contains(&control.semantic) {
            return Err(MtgoContractErrorV1::new(
                "visible_control_semantic_not_legal",
                &control.control_id,
            ));
        }
        let rect = exact_current_frame_region_v1(
            decision,
            control.frame_region_evidence_id,
            "visible_control_evidence_invalid",
        )?;
        if rects_intersect_v1(rect, prompt_region) {
            return Err(MtgoContractErrorV1::new(
                "visible_control_overlaps_prompt_region",
                &control.control_id,
            ));
        }
        if &control.semantic == selection.selected_semantic() {
            selected_matches.push((control, rect));
        }
    }
    if selected_matches.len() != 1 {
        return Err(MtgoContractErrorV1::new(
            "visible_control_selected_match_count",
            selected_matches.len().to_string(),
        ));
    }
    let (selected, rect_client_px) = selected_matches[0];

    #[derive(Serialize)]
    struct ResolutionCommitmentRecordV1<'a> {
        control_set: &'a MtgoVisibleActionControlSetV1,
        selection_commitment_sha256: &'a str,
        selected_control_id: &'a str,
        selected_rect_client_px: &'a MtgoRectPxV1,
    }
    let encoded = serde_json::to_vec(&ResolutionCommitmentRecordV1 {
        control_set: &control_set,
        selection_commitment_sha256: selection.selection_commitment_sha256(),
        selected_control_id: &selected.control_id,
        selected_rect_client_px: rect_client_px,
    })
    .map_err(|error| {
        MtgoContractErrorV1::new(
            "visible_control_resolution_serialization_failed",
            error.to_string(),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(RESOLUTION_COMMITMENT_DOMAIN_V1);
    hasher.update((encoded.len() as u64).to_le_bytes());
    hasher.update(encoded);
    let resolution_commitment_sha256 = format!("{:x}", hasher.finalize());

    Ok(CheckedUntrustedMtgoResolvedActionControlV1 {
        decision_commitment_sha256: decision.decision_commitment_sha256().to_owned(),
        selection_commitment_sha256: selection.selection_commitment_sha256().to_owned(),
        control_id: selected.control_id.clone(),
        frame_id: control_set.frame_id,
        frame_sequence: control_set.frame_sequence,
        frame_region_evidence_id: selected.frame_region_evidence_id,
        rect_client_px: rect_client_px.clone(),
        resolution_commitment_sha256,
    })
}

fn exact_current_frame_region_v1<'a>(
    decision: &'a ValidatedMtgoObservedDecisionV1,
    evidence_id: u64,
    code: &'static str,
) -> Result<&'a MtgoRectPxV1, MtgoContractErrorV1> {
    let evidence = decision
        .record
        .evidence
        .iter()
        .find(|item| item.evidence_id == evidence_id)
        .ok_or_else(|| MtgoContractErrorV1::new(code, evidence_id.to_string()))?;
    let MtgoEvidenceSourceV1::FrameRegion { frame_id, rect, .. } = &evidence.source else {
        return Err(MtgoContractErrorV1::new(code, evidence_id.to_string()));
    };
    if *frame_id != decision.frame_id() {
        return Err(MtgoContractErrorV1::new(code, evidence_id.to_string()));
    }
    Ok(rect)
}

fn rects_intersect_v1(left: &MtgoRectPxV1, right: &MtgoRectPxV1) -> bool {
    let left_right = u64::from(left.x) + u64::from(left.width);
    let left_bottom = u64::from(left.y) + u64::from(left.height);
    let right_right = u64::from(right.x) + u64::from(right.width);
    let right_bottom = u64::from(right.y) + u64::from(right.height);
    u64::from(left.x) < right_right
        && u64::from(right.x) < left_right
        && u64::from(left.y) < right_bottom
        && u64::from(right.y) < left_bottom
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

fn validate_safe_identifier_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(MtgoContractErrorV1::new(code, value));
    }
    Ok(())
}
