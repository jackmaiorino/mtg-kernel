use crate::{
    CheckedUntrustedMtgoGameplayCalibrationV1,
    CheckedUntrustedMtgoProfileBoundResolvedActionControlV1,
    CheckedUntrustedMtgoVisibleObjectActionCalibrationV1, MtgoContractErrorV1,
    MtgoDxgiCaptureRoleV2, MtgoEvidenceSourceV1, MtgoGameplayVisibleChangeV1, MtgoRectPxV1,
    MtgoSizePxV1, MtgoVisibleObjectCalibrationActionV1,
};
use mtg_kernel::rl::ActionSemanticV1;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_PROFILE_BOUND_POSTCONDITION_REGION_SET_SCHEMA_V1: u32 = 1;
pub const MTGO_PROFILE_BOUND_POSTCONDITION_AFTER_FRAME_SCHEMA_V1: u32 = 1;

const POSTCONDITION_PLAN_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-profile-bound-action-postcondition-plan-v1";
const POSTCONDITION_CONFIRMATION_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-profile-bound-action-postcondition-confirmation-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoDuelVisiblePostconditionKindV1 {
    SelectedControlChanged,
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
pub struct MtgoProfileBoundPostconditionRegionCandidateV1 {
    pub kind: MtgoDuelVisiblePostconditionKindV1,
    pub frame_region_evidence_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoProfileBoundPostconditionRegionSetV1 {
    pub schema_version: u32,
    pub profile_bound_resolution_commitment_sha256: String,
    pub decision_commitment_sha256: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub candidate_set_complete: bool,
    pub regions: Vec<MtgoProfileBoundPostconditionRegionCandidateV1>,
}

pub enum MtgoProfileBoundPostconditionCalibrationV1 {
    Gameplay(CheckedUntrustedMtgoGameplayCalibrationV1),
    VisibleObject(CheckedUntrustedMtgoVisibleObjectActionCalibrationV1),
}

impl From<CheckedUntrustedMtgoGameplayCalibrationV1>
    for MtgoProfileBoundPostconditionCalibrationV1
{
    fn from(value: CheckedUntrustedMtgoGameplayCalibrationV1) -> Self {
        Self::Gameplay(value)
    }
}

impl From<CheckedUntrustedMtgoVisibleObjectActionCalibrationV1>
    for MtgoProfileBoundPostconditionCalibrationV1
{
    fn from(value: CheckedUntrustedMtgoVisibleObjectActionCalibrationV1) -> Self {
        Self::VisibleObject(value)
    }
}

#[derive(Serialize)]
struct PrivatePostconditionRegionV1 {
    kind: MtgoDuelVisiblePostconditionKindV1,
    frame_region_evidence_id: u64,
    rect_client_px: MtgoRectPxV1,
    before_bgra8_sha256: String,
}

/// A move-only declaration of every calibrated visible region that must change
/// after one exact source, profile, deployment, model-selection, and control
/// resolution chain.
///
/// Regions and their before hashes remain private. This type cannot create an
/// input command or competitive entry.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoProfileBoundActionPostconditionPlanV1;
/// fn cannot_act(value: &CheckedUntrustedMtgoProfileBoundActionPostconditionPlanV1) {
///     let _ = value.regions();
///     let _ = value.input_command();
/// }
/// ```
pub struct CheckedUntrustedMtgoProfileBoundActionPostconditionPlanV1 {
    resolution: CheckedUntrustedMtgoProfileBoundResolvedActionControlV1,
    _calibration: MtgoProfileBoundPostconditionCalibrationV1,
    regions: Vec<PrivatePostconditionRegionV1>,
    source_manifest_sha256: String,
    source_frame_sha256: String,
    source_output_identity_sha256: String,
    source_client_size_px: MtgoSizePxV1,
    plan_commitment_sha256: String,
}

impl CheckedUntrustedMtgoProfileBoundActionPostconditionPlanV1 {
    pub fn profile_bound_resolution_commitment_sha256(&self) -> &str {
        self.resolution.profile_bound_resolution_commitment_sha256()
    }

    pub fn plan_commitment_sha256(&self) -> &str {
        &self.plan_commitment_sha256
    }

    pub fn selected_semantic(&self) -> &ActionSemanticV1 {
        self.resolution.selected_semantic()
    }

    pub fn required_region_count(&self) -> usize {
        self.regions.len()
    }

    pub fn safe_for_live_input(&self) -> bool {
        false
    }

    pub fn permits_match_entry(&self) -> bool {
        false
    }

    pub(crate) fn source_frame_id_v1(&self) -> u64 {
        self.resolution.frame_id()
    }

    pub(crate) fn source_frame_sequence_v1(&self) -> u64 {
        self.resolution.frame_sequence()
    }

    pub(crate) fn source_frame_sha256_v1(&self) -> &str {
        &self.source_frame_sha256
    }

    pub(crate) fn source_client_size_px_v1(&self) -> &MtgoSizePxV1 {
        &self.source_client_size_px
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoProfileBoundPostconditionAfterRegionV1 {
    pub kind: MtgoDuelVisiblePostconditionKindV1,
    pub rect_client_px: MtgoRectPxV1,
    pub after_bgra8_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoProfileBoundPostconditionAfterFrameV1 {
    pub schema_version: u32,
    pub plan_commitment_sha256: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub manifest_sha256: String,
    pub canonical_bgra8_sha256: String,
    pub output_identity_sha256: String,
    pub perception_profile_admission_commitment_sha256: String,
    pub client_size_px: MtgoSizePxV1,
    pub capture_role: MtgoDxgiCaptureRoleV2,
    pub regions: Vec<MtgoProfileBoundPostconditionAfterRegionV1>,
}

/// Structurally confirmed action-specific visible changes on a newer frame.
///
/// The producer-facing after-frame record is not a trusted capture proof, so
/// this wrapper remains checked-untrusted and never authorizes another input.
/// A live implementation must perform the same checks while retaining the
/// opaque in-process DXGI before and after frames.
pub struct CheckedUntrustedMtgoProfileBoundActionPostconditionV1 {
    _plan: CheckedUntrustedMtgoProfileBoundActionPostconditionPlanV1,
    after_frame_id: u64,
    after_frame_sequence: u64,
    confirmation_commitment_sha256: String,
}

impl CheckedUntrustedMtgoProfileBoundActionPostconditionV1 {
    pub fn after_frame_id(&self) -> u64 {
        self.after_frame_id
    }

    pub fn after_frame_sequence(&self) -> u64 {
        self.after_frame_sequence
    }

    pub fn confirmation_commitment_sha256(&self) -> &str {
        &self.confirmation_commitment_sha256
    }

    pub fn safe_for_additional_input(&self) -> bool {
        false
    }

    pub fn permits_match_entry(&self) -> bool {
        false
    }
}

pub fn prepare_profile_bound_action_postcondition_plan_v1(
    resolution: CheckedUntrustedMtgoProfileBoundResolvedActionControlV1,
    calibration: MtgoProfileBoundPostconditionCalibrationV1,
    region_set: MtgoProfileBoundPostconditionRegionSetV1,
) -> Result<CheckedUntrustedMtgoProfileBoundActionPostconditionPlanV1, MtgoContractErrorV1> {
    if region_set.schema_version != MTGO_PROFILE_BOUND_POSTCONDITION_REGION_SET_SCHEMA_V1 {
        return Err(error_v1(
            "profile_bound_postcondition_region_set_schema",
            region_set.schema_version.to_string(),
        ));
    }
    require_sha256_v1(
        &region_set.profile_bound_resolution_commitment_sha256,
        "profile_bound_postcondition_resolution_hash",
    )?;
    if region_set.profile_bound_resolution_commitment_sha256
        != resolution.profile_bound_resolution_commitment_sha256()
        || region_set.decision_commitment_sha256 != resolution.decision_commitment_sha256_v1()
    {
        return Err(error_v1(
            "profile_bound_postcondition_source_mismatch",
            "region set must bind the exact control resolution and decision",
        ));
    }
    if region_set.frame_id != resolution.frame_id()
        || region_set.frame_sequence != resolution.frame_sequence()
    {
        return Err(error_v1(
            "profile_bound_postcondition_stale_frame",
            "region set must bind the exact current decision frame",
        ));
    }
    if !region_set.candidate_set_complete {
        return Err(error_v1(
            "profile_bound_postcondition_region_set_incomplete",
            "the action-specific visible region set must be complete",
        ));
    }

    let decision = resolution.validated_decision_v1();
    let source_frame = decision
        .record
        .frames
        .iter()
        .find(|frame| frame.frame_id == decision.frame_id())
        .ok_or_else(|| {
            error_v1(
                "profile_bound_postcondition_source_frame_missing",
                decision.frame_id().to_string(),
            )
        })?;
    let calibration_regions = calibration_regions_v1(&calibration, resolution.selected_semantic())?;
    if calibration_client_size_v1(&calibration)
        != (
            source_frame.client_bounds.width,
            source_frame.client_bounds.height,
        )
    {
        return Err(error_v1(
            "profile_bound_postcondition_calibration_geometry",
            "calibration and current decision client sizes differ",
        ));
    }
    if region_set.regions.len() != calibration_regions.len() {
        return Err(error_v1(
            "profile_bound_postcondition_region_count",
            format!(
                "expected {}, found {}",
                calibration_regions.len(),
                region_set.regions.len()
            ),
        ));
    }

    let selected_rect = resolution.selected_rect_client_px_v1().clone();
    let selected_hash =
        exact_current_frame_region_v1(decision, resolution.selected_frame_region_evidence_id_v1())?
            .1
            .to_owned();
    let mut private_regions = vec![PrivatePostconditionRegionV1 {
        kind: MtgoDuelVisiblePostconditionKindV1::SelectedControlChanged,
        frame_region_evidence_id: resolution.selected_frame_region_evidence_id_v1(),
        rect_client_px: selected_rect,
        before_bgra8_sha256: selected_hash,
    }];
    let mut evidence_ids = HashSet::new();
    evidence_ids.insert(resolution.selected_frame_region_evidence_id_v1());
    for (candidate, expected) in region_set.regions.iter().zip(calibration_regions) {
        if candidate.kind != expected.0 {
            return Err(error_v1(
                "profile_bound_postcondition_region_kind",
                format!("expected {:?}, found {:?}", expected.0, candidate.kind),
            ));
        }
        if !evidence_ids.insert(candidate.frame_region_evidence_id) {
            return Err(error_v1(
                "profile_bound_postcondition_region_evidence_duplicate",
                candidate.frame_region_evidence_id.to_string(),
            ));
        }
        let (rect, content_sha256) =
            exact_current_frame_region_v1(decision, candidate.frame_region_evidence_id)?;
        if rect != expected.1 {
            return Err(error_v1(
                "profile_bound_postcondition_region_geometry",
                format!("{:?}", candidate.kind),
            ));
        }
        private_regions.push(PrivatePostconditionRegionV1 {
            kind: candidate.kind,
            frame_region_evidence_id: candidate.frame_region_evidence_id,
            rect_client_px: rect.clone(),
            before_bgra8_sha256: content_sha256.to_owned(),
        });
    }

    let regions_bytes = serde_json::to_vec(&private_regions).map_err(|error| {
        error_v1(
            "profile_bound_postcondition_plan_serialization",
            error.to_string(),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(POSTCONDITION_PLAN_COMMITMENT_DOMAIN_V1);
    for part in [
        resolution
            .profile_bound_resolution_commitment_sha256()
            .as_bytes(),
        calibration_commitment_v1(&calibration).as_bytes(),
        resolution.source_candidate_commitment_sha256().as_bytes(),
        resolution
            .perception_profile_admission_commitment_sha256()
            .as_bytes(),
        regions_bytes.as_slice(),
        b"checked_untrusted_no_input_or_event_entry",
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }

    Ok(CheckedUntrustedMtgoProfileBoundActionPostconditionPlanV1 {
        source_manifest_sha256: resolution.source_manifest_sha256_v1().to_owned(),
        source_frame_sha256: resolution.source_canonical_bgra8_sha256_v1().to_owned(),
        source_output_identity_sha256: resolution.source_output_identity_sha256_v1().to_owned(),
        source_client_size_px: MtgoSizePxV1 {
            width: source_frame.client_bounds.width,
            height: source_frame.client_bounds.height,
        },
        resolution,
        _calibration: calibration,
        regions: private_regions,
        plan_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

pub fn check_untrusted_profile_bound_action_postcondition_v1(
    plan: CheckedUntrustedMtgoProfileBoundActionPostconditionPlanV1,
    after: MtgoProfileBoundPostconditionAfterFrameV1,
) -> Result<CheckedUntrustedMtgoProfileBoundActionPostconditionV1, MtgoContractErrorV1> {
    if after.schema_version != MTGO_PROFILE_BOUND_POSTCONDITION_AFTER_FRAME_SCHEMA_V1 {
        return Err(error_v1(
            "profile_bound_postcondition_after_schema",
            after.schema_version.to_string(),
        ));
    }
    for (value, code) in [
        (
            after.plan_commitment_sha256.as_str(),
            "profile_bound_postcondition_after_plan_hash",
        ),
        (
            after.manifest_sha256.as_str(),
            "profile_bound_postcondition_after_manifest_hash",
        ),
        (
            after.canonical_bgra8_sha256.as_str(),
            "profile_bound_postcondition_after_frame_hash",
        ),
        (
            after.output_identity_sha256.as_str(),
            "profile_bound_postcondition_after_output_hash",
        ),
        (
            after
                .perception_profile_admission_commitment_sha256
                .as_str(),
            "profile_bound_postcondition_after_profile_hash",
        ),
    ] {
        require_sha256_v1(value, code)?;
    }
    if after.plan_commitment_sha256 != plan.plan_commitment_sha256 {
        return Err(error_v1(
            "profile_bound_postcondition_after_plan_mismatch",
            "after frame must bind the exact pre-input plan",
        ));
    }
    if after.frame_id == plan.resolution.frame_id()
        || after.frame_sequence <= plan.resolution.frame_sequence()
        || after.manifest_sha256 == plan.source_manifest_sha256
        || after.canonical_bgra8_sha256 == plan.source_frame_sha256
    {
        return Err(error_v1(
            "profile_bound_postcondition_after_not_newer",
            "after frame must be strictly newer and byte-distinct",
        ));
    }
    if after.output_identity_sha256 != plan.source_output_identity_sha256 {
        return Err(error_v1(
            "profile_bound_postcondition_after_output_mismatch",
            "before and after frames must share output identity",
        ));
    }
    if after.perception_profile_admission_commitment_sha256
        != plan
            .resolution
            .perception_profile_admission_commitment_sha256()
        || after.client_size_px != plan.source_client_size_px
        || after.capture_role != MtgoDxgiCaptureRoleV2::ActingPlayerDuel
    {
        return Err(error_v1(
            "profile_bound_postcondition_after_runtime_mismatch",
            "after frame must retain the exact admitted duel profile, geometry, and role",
        ));
    }
    if after.regions.len() != plan.regions.len() {
        return Err(error_v1(
            "profile_bound_postcondition_after_region_count",
            format!(
                "expected {}, found {}",
                plan.regions.len(),
                after.regions.len()
            ),
        ));
    }
    for (observed, required) in after.regions.iter().zip(&plan.regions) {
        require_sha256_v1(
            &observed.after_bgra8_sha256,
            "profile_bound_postcondition_after_region_hash",
        )?;
        if observed.kind != required.kind || observed.rect_client_px != required.rect_client_px {
            return Err(error_v1(
                "profile_bound_postcondition_after_region_mismatch",
                format!("{:?}", observed.kind),
            ));
        }
        if observed.after_bgra8_sha256 == required.before_bgra8_sha256 {
            return Err(error_v1(
                "profile_bound_postcondition_after_region_unchanged",
                format!("{:?}", observed.kind),
            ));
        }
    }

    let after_bytes = serde_json::to_vec(&after).map_err(|error| {
        error_v1(
            "profile_bound_postcondition_after_serialization",
            error.to_string(),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(POSTCONDITION_CONFIRMATION_COMMITMENT_DOMAIN_V1);
    for part in [
        plan.plan_commitment_sha256.as_bytes(),
        after_bytes.as_slice(),
        b"checked_untrusted_no_additional_input_or_event_entry",
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    Ok(CheckedUntrustedMtgoProfileBoundActionPostconditionV1 {
        _plan: plan,
        after_frame_id: after.frame_id,
        after_frame_sequence: after.frame_sequence,
        confirmation_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

fn calibration_regions_v1<'a>(
    calibration: &'a MtgoProfileBoundPostconditionCalibrationV1,
    selected: &ActionSemanticV1,
) -> Result<Vec<(MtgoDuelVisiblePostconditionKindV1, &'a MtgoRectPxV1)>, MtgoContractErrorV1> {
    let regions = match calibration {
        MtgoProfileBoundPostconditionCalibrationV1::Gameplay(calibration) => {
            if calibration.action() != selected {
                return Err(error_v1(
                    "profile_bound_postcondition_calibration_action",
                    "gameplay calibration does not match the selected semantic",
                ));
            }
            calibration
                .visible_postconditions_v1()
                .iter()
                .map(|region| (map_change_v1(region.change), &region.rect_client_px))
                .collect()
        }
        MtgoProfileBoundPostconditionCalibrationV1::VisibleObject(calibration) => {
            if !visible_object_calibration_matches_v1(calibration.action(), selected) {
                return Err(error_v1(
                    "profile_bound_postcondition_calibration_action",
                    "visible-object calibration does not match the selected action family",
                ));
            }
            calibration
                .visible_postconditions_v1()
                .iter()
                .map(|region| (map_change_v1(region.change), &region.rect_client_px))
                .collect()
        }
    };
    Ok(regions)
}

fn visible_object_calibration_matches_v1(
    calibration: &MtgoVisibleObjectCalibrationActionV1,
    selected: &ActionSemanticV1,
) -> bool {
    match (calibration, selected) {
        (
            MtgoVisibleObjectCalibrationActionV1::PlayLand { actor: left, .. },
            ActionSemanticV1::PlayLand { actor: right, .. },
        ) => left == right,
        (
            MtgoVisibleObjectCalibrationActionV1::ActivateManaAbility {
                actor: left,
                mana_choice: left_choice,
                ..
            },
            ActionSemanticV1::ActivateManaAbility {
                actor: right,
                mana_choice: right_choice,
                ..
            },
        ) => left == right && left_choice == right_choice,
        _ => false,
    }
}

fn calibration_client_size_v1(
    calibration: &MtgoProfileBoundPostconditionCalibrationV1,
) -> (u32, u32) {
    let size = match calibration {
        MtgoProfileBoundPostconditionCalibrationV1::Gameplay(value) => value.client_size_px(),
        MtgoProfileBoundPostconditionCalibrationV1::VisibleObject(value) => value.client_size_px(),
    };
    (size.width, size.height)
}

fn calibration_commitment_v1(calibration: &MtgoProfileBoundPostconditionCalibrationV1) -> &str {
    match calibration {
        MtgoProfileBoundPostconditionCalibrationV1::Gameplay(value) => {
            value.transition_commitment_sha256()
        }
        MtgoProfileBoundPostconditionCalibrationV1::VisibleObject(value) => {
            value.transition_commitment_sha256()
        }
    }
}

fn map_change_v1(change: MtgoGameplayVisibleChangeV1) -> MtgoDuelVisiblePostconditionKindV1 {
    match change {
        MtgoGameplayVisibleChangeV1::PromptChanged => {
            MtgoDuelVisiblePostconditionKindV1::PromptChanged
        }
        MtgoGameplayVisibleChangeV1::PhaseBarChanged => {
            MtgoDuelVisiblePostconditionKindV1::PhaseBarChanged
        }
        MtgoGameplayVisibleChangeV1::PlayerCountsChanged => {
            MtgoDuelVisiblePostconditionKindV1::PlayerCountsChanged
        }
        MtgoGameplayVisibleChangeV1::BattlefieldChanged => {
            MtgoDuelVisiblePostconditionKindV1::BattlefieldChanged
        }
        MtgoGameplayVisibleChangeV1::HandChanged => MtgoDuelVisiblePostconditionKindV1::HandChanged,
        MtgoGameplayVisibleChangeV1::VisibleGameLogChanged => {
            MtgoDuelVisiblePostconditionKindV1::VisibleGameLogChanged
        }
        MtgoGameplayVisibleChangeV1::ManaPoolChanged => {
            MtgoDuelVisiblePostconditionKindV1::ManaPoolChanged
        }
    }
}

fn exact_current_frame_region_v1(
    decision: &crate::ValidatedMtgoObservedDecisionV1,
    evidence_id: u64,
) -> Result<(&MtgoRectPxV1, &str), MtgoContractErrorV1> {
    let evidence = decision
        .record
        .evidence
        .iter()
        .find(|item| item.evidence_id == evidence_id)
        .ok_or_else(|| {
            error_v1(
                "profile_bound_postcondition_region_evidence",
                evidence_id.to_string(),
            )
        })?;
    let MtgoEvidenceSourceV1::FrameRegion {
        frame_id,
        rect,
        content_sha256,
    } = &evidence.source
    else {
        return Err(error_v1(
            "profile_bound_postcondition_region_evidence",
            evidence_id.to_string(),
        ));
    };
    if *frame_id != decision.frame_id() {
        return Err(error_v1(
            "profile_bound_postcondition_region_evidence",
            evidence_id.to_string(),
        ));
    }
    require_sha256_v1(
        content_sha256,
        "profile_bound_postcondition_region_content_hash",
    )?;
    Ok((rect, content_sha256))
}

fn require_sha256_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(code, value));
    }
    Ok(())
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        profile_bound_resolved_action_control_for_test_v1,
        validate_visible_object_action_calibration_trace_v1,
        MtgoVisibleObjectActionCalibrationTraceV1,
    };

    fn region_set_v1(
        resolution: &CheckedUntrustedMtgoProfileBoundResolvedActionControlV1,
    ) -> MtgoProfileBoundPostconditionRegionSetV1 {
        MtgoProfileBoundPostconditionRegionSetV1 {
            schema_version: MTGO_PROFILE_BOUND_POSTCONDITION_REGION_SET_SCHEMA_V1,
            profile_bound_resolution_commitment_sha256: resolution
                .profile_bound_resolution_commitment_sha256()
                .to_owned(),
            decision_commitment_sha256: resolution.decision_commitment_sha256_v1().to_owned(),
            frame_id: resolution.frame_id(),
            frame_sequence: resolution.frame_sequence(),
            candidate_set_complete: true,
            regions: vec![
                region_v1(MtgoDuelVisiblePostconditionKindV1::PromptChanged, 60),
                region_v1(MtgoDuelVisiblePostconditionKindV1::PlayerCountsChanged, 70),
                region_v1(MtgoDuelVisiblePostconditionKindV1::BattlefieldChanged, 80),
                region_v1(MtgoDuelVisiblePostconditionKindV1::HandChanged, 90),
                region_v1(
                    MtgoDuelVisiblePostconditionKindV1::VisibleGameLogChanged,
                    100,
                ),
            ],
        }
    }

    fn region_v1(
        kind: MtgoDuelVisiblePostconditionKindV1,
        frame_region_evidence_id: u64,
    ) -> MtgoProfileBoundPostconditionRegionCandidateV1 {
        MtgoProfileBoundPostconditionRegionCandidateV1 {
            kind,
            frame_region_evidence_id,
        }
    }

    fn calibration_v1() -> MtgoProfileBoundPostconditionCalibrationV1 {
        let record: MtgoVisibleObjectActionCalibrationTraceV1 = serde_json::from_str(include_str!(
            "../fixtures/solitaire_play_land_transition_v1.json"
        ))
        .unwrap();
        validate_visible_object_action_calibration_trace_v1(record)
            .unwrap()
            .into()
    }

    fn after_frame_v1(
        plan: &CheckedUntrustedMtgoProfileBoundActionPostconditionPlanV1,
    ) -> MtgoProfileBoundPostconditionAfterFrameV1 {
        MtgoProfileBoundPostconditionAfterFrameV1 {
            schema_version: MTGO_PROFILE_BOUND_POSTCONDITION_AFTER_FRAME_SCHEMA_V1,
            plan_commitment_sha256: plan.plan_commitment_sha256().to_owned(),
            frame_id: plan.resolution.frame_id() + 1,
            frame_sequence: plan.resolution.frame_sequence() + 1,
            manifest_sha256: "a".repeat(64),
            canonical_bgra8_sha256: "b".repeat(64),
            output_identity_sha256: plan.source_output_identity_sha256.clone(),
            perception_profile_admission_commitment_sha256: plan
                .resolution
                .perception_profile_admission_commitment_sha256()
                .to_owned(),
            client_size_px: plan.source_client_size_px.clone(),
            capture_role: MtgoDxgiCaptureRoleV2::ActingPlayerDuel,
            regions: plan
                .regions
                .iter()
                .enumerate()
                .map(
                    |(index, region)| MtgoProfileBoundPostconditionAfterRegionV1 {
                        kind: region.kind,
                        rect_client_px: region.rect_client_px.clone(),
                        after_bgra8_sha256: format!("{:064x}", index + 100),
                    },
                )
                .collect(),
        }
    }

    #[test]
    fn play_land_requires_control_prompt_counts_battlefield_hand_and_log_changes() {
        let resolution = profile_bound_resolved_action_control_for_test_v1();
        let region_set = region_set_v1(&resolution);
        let plan = prepare_profile_bound_action_postcondition_plan_v1(
            resolution,
            calibration_v1(),
            region_set,
        )
        .unwrap();
        assert_eq!(plan.required_region_count(), 6);
        assert!(!plan.safe_for_live_input());
        assert!(!plan.permits_match_entry());
        let after = after_frame_v1(&plan);
        let confirmed = check_untrusted_profile_bound_action_postcondition_v1(plan, after).unwrap();
        assert_eq!(confirmed.after_frame_id(), 2);
        assert_eq!(confirmed.confirmation_commitment_sha256().len(), 64);
        assert!(!confirmed.safe_for_additional_input());
        assert!(!confirmed.permits_match_entry());
    }

    #[test]
    fn incomplete_wrong_action_or_stale_plan_rejects() {
        let resolution = profile_bound_resolved_action_control_for_test_v1();
        let mut incomplete = region_set_v1(&resolution);
        incomplete.candidate_set_complete = false;
        assert_eq!(
            prepare_profile_bound_action_postcondition_plan_v1(
                resolution,
                calibration_v1(),
                incomplete,
            )
            .err()
            .unwrap()
            .code(),
            "profile_bound_postcondition_region_set_incomplete"
        );

        let resolution = profile_bound_resolved_action_control_for_test_v1();
        let region_set = region_set_v1(&resolution);
        let gameplay = crate::validate_gameplay_calibration_trace_v1(
            serde_json::from_str(include_str!(
                "../fixtures/solitaire_pass_to_combat_transition_v1.json"
            ))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            prepare_profile_bound_action_postcondition_plan_v1(
                resolution,
                gameplay.into(),
                region_set,
            )
            .err()
            .unwrap()
            .code(),
            "profile_bound_postcondition_calibration_action"
        );

        let resolution = profile_bound_resolved_action_control_for_test_v1();
        let region_set = region_set_v1(&resolution);
        let plan = prepare_profile_bound_action_postcondition_plan_v1(
            resolution,
            calibration_v1(),
            region_set,
        )
        .unwrap();
        let mut after = after_frame_v1(&plan);
        after.frame_sequence = plan.resolution.frame_sequence();
        assert_eq!(
            check_untrusted_profile_bound_action_postcondition_v1(plan, after)
                .err()
                .unwrap()
                .code(),
            "profile_bound_postcondition_after_not_newer"
        );
    }

    #[test]
    fn missing_reordered_or_unchanged_required_region_rejects() {
        let resolution = profile_bound_resolved_action_control_for_test_v1();
        let mut missing = region_set_v1(&resolution);
        missing.regions.pop();
        assert_eq!(
            prepare_profile_bound_action_postcondition_plan_v1(
                resolution,
                calibration_v1(),
                missing,
            )
            .err()
            .unwrap()
            .code(),
            "profile_bound_postcondition_region_count"
        );

        let resolution = profile_bound_resolved_action_control_for_test_v1();
        let mut reordered = region_set_v1(&resolution);
        reordered.regions.swap(0, 1);
        assert_eq!(
            prepare_profile_bound_action_postcondition_plan_v1(
                resolution,
                calibration_v1(),
                reordered,
            )
            .err()
            .unwrap()
            .code(),
            "profile_bound_postcondition_region_kind"
        );

        let resolution = profile_bound_resolved_action_control_for_test_v1();
        let region_set = region_set_v1(&resolution);
        let plan = prepare_profile_bound_action_postcondition_plan_v1(
            resolution,
            calibration_v1(),
            region_set,
        )
        .unwrap();
        let mut after = after_frame_v1(&plan);
        after.regions[3].after_bgra8_sha256 = plan.regions[3].before_bgra8_sha256.clone();
        assert_eq!(
            check_untrusted_profile_bound_action_postcondition_v1(plan, after)
                .err()
                .unwrap()
                .code(),
            "profile_bound_postcondition_after_region_unchanged"
        );
    }

    #[test]
    fn after_frame_profile_geometry_role_and_output_must_remain_exact() {
        for mutation in 0..4 {
            let resolution = profile_bound_resolved_action_control_for_test_v1();
            let region_set = region_set_v1(&resolution);
            let plan = prepare_profile_bound_action_postcondition_plan_v1(
                resolution,
                calibration_v1(),
                region_set,
            )
            .unwrap();
            let mut after = after_frame_v1(&plan);
            match mutation {
                0 => after.perception_profile_admission_commitment_sha256 = "e".repeat(64),
                1 => after.client_size_px.width -= 1,
                2 => after.capture_role = MtgoDxgiCaptureRoleV2::Navigation,
                3 => after.output_identity_sha256 = "f".repeat(64),
                _ => unreachable!(),
            }
            let code = check_untrusted_profile_bound_action_postcondition_v1(plan, after)
                .err()
                .unwrap()
                .code();
            if mutation == 3 {
                assert_eq!(code, "profile_bound_postcondition_after_output_mismatch");
            } else {
                assert_eq!(code, "profile_bound_postcondition_after_runtime_mismatch");
            }
        }
    }
}
