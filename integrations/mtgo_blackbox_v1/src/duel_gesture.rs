use crate::{
    duel_action_family_v1, CheckedUntrustedMtgoProfileBoundResolvedActionControlV1,
    MtgoContractErrorV1, MtgoDuelActionFamilyV1, MtgoEvidenceSourceV1, MtgoRectPxV1,
    ValidatedMtgoObservedDecisionV1, MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1,
};
use mtg_kernel::rl::{ActionSemanticV1, CardStableRefV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_DUEL_GESTURE_PLAN_SCHEMA_V1: u32 = 1;
pub const MTGO_DUEL_GESTURE_TARGET_SET_SCHEMA_V1: u32 = 1;

const DUEL_GESTURE_PLAN_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-duel-gesture-plan-v1";
const DUEL_GESTURE_STAGE_BINDING_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-duel-gesture-stage-visible-binding-v1";
const MAX_DUEL_GESTURE_STAGES_V1: usize = 32;
const MAX_DUEL_GESTURE_TARGETS_V1: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoDuelGestureFrameBindingV1 {
    SourceDecisionFrame,
    StrictlyNewerVisibleContinuation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoDuelGestureExpectedEffectV1 {
    ContinueSelectedSemantic,
    CompleteSelectedSemantic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoDuelPrimaryActivationV1 {
    SingleLeftClick,
    DoubleLeftClick,
    RightClick,
}

/// A semantic gesture primitive. It deliberately contains no coordinates,
/// device handle, timing, key code, or input API. A future Windows binder must
/// resolve every primitive against a fresh admitted visible frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "primitive_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoDuelGesturePrimitiveV1 {
    ActivatePrimary {
        activation: MtgoDuelPrimaryActivationV1,
    },
    ActivateSemanticMenuChoice {
        activation: MtgoDuelPrimaryActivationV1,
    },
    DragPrimaryToCalibratedPlayArea,
    SelectObject {
        object: CardStableRefV1,
    },
    DragObjectToOrderSlot {
        object: CardStableRefV1,
        slot_index: u16,
    },
    Submit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDuelGestureStageV1 {
    pub stage_index: u16,
    pub frame_binding: MtgoDuelGestureFrameBindingV1,
    pub primitive: MtgoDuelGesturePrimitiveV1,
    pub expected_effect: MtgoDuelGestureExpectedEffectV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDuelGesturePlanV1 {
    pub schema_version: u32,
    pub profile_bound_resolution_commitment_sha256: String,
    pub decision_commitment_sha256: String,
    pub selection_commitment_sha256: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub selected_action_family: MtgoDuelActionFamilyV1,
    pub stage_set_complete: bool,
    pub stages: Vec<MtgoDuelGestureStageV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoDuelGesturePlanCommitmentsV1 {
    pub profile_bound_resolution_commitment_sha256: String,
    pub decision_commitment_sha256: String,
    pub selection_commitment_sha256: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub selected_action_family: MtgoDuelActionFamilyV1,
    pub stage_count: u16,
    pub plan_commitment_sha256: String,
}

/// A structurally checked, move-only gesture blueprint for one exact selected
/// legal action. This value has no coordinates or input authority. The first
/// stage is tied to the source decision frame, and every later stage requires
/// a fresh visible continuation frame before its one primitive may be bound.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoDuelGesturePlanV1;
/// fn cannot_act(value: &CheckedUntrustedMtgoDuelGesturePlanV1) {
///     let _ = value.coordinates();
///     let _ = value.send_input();
/// }
/// ```
pub struct CheckedUntrustedMtgoDuelGesturePlanV1 {
    plan: MtgoDuelGesturePlanV1,
    selected_semantic: ActionSemanticV1,
    plan_commitment_sha256: String,
}

impl CheckedUntrustedMtgoDuelGesturePlanV1 {
    pub fn commitments_v1(&self) -> MtgoDuelGesturePlanCommitmentsV1 {
        MtgoDuelGesturePlanCommitmentsV1 {
            profile_bound_resolution_commitment_sha256: self
                .plan
                .profile_bound_resolution_commitment_sha256
                .clone(),
            decision_commitment_sha256: self.plan.decision_commitment_sha256.clone(),
            selection_commitment_sha256: self.plan.selection_commitment_sha256.clone(),
            frame_id: self.plan.frame_id,
            frame_sequence: self.plan.frame_sequence,
            selected_action_family: self.plan.selected_action_family,
            stage_count: self.plan.stages.len() as u16,
            plan_commitment_sha256: self.plan_commitment_sha256.clone(),
        }
    }

    pub fn safe_for_live_input(&self) -> bool {
        false
    }

    pub fn permits_event_entry(&self) -> bool {
        false
    }

    pub fn stages_v1(&self) -> &[MtgoDuelGestureStageV1] {
        &self.plan.stages
    }

    pub fn selected_semantic_v1(&self) -> &ActionSemanticV1 {
        &self.selected_semantic
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "target_role", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoDuelGestureTargetRoleV1 {
    PrimarySemanticControl,
    SemanticMenuChoice,
    SemanticObject { object: CardStableRefV1 },
    CalibratedPlayArea,
    OrderSlot { slot_index: u16 },
    SubmitControl,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleDuelGestureTargetCandidateV1 {
    pub target_id: String,
    pub role: MtgoDuelGestureTargetRoleV1,
    pub frame_region_evidence_id: u64,
    pub confidence_bps: u16,
    pub visibly_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleDuelGestureTargetSetV1 {
    pub schema_version: u32,
    pub gesture_plan_commitment_sha256: String,
    pub decision_commitment_sha256: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub stage_index: u16,
    pub candidate_set_complete: bool,
    pub targets: Vec<MtgoVisibleDuelGestureTargetCandidateV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoDuelGestureStageBindingCommitmentsV1 {
    pub gesture_plan_commitment_sha256: String,
    pub decision_commitment_sha256: String,
    pub stage_index: u16,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub target_count: u16,
    pub binding_commitment_sha256: String,
}

#[derive(Serialize)]
struct PrivateVisibleGestureTargetV1 {
    target_id: String,
    role: MtgoDuelGestureTargetRoleV1,
    frame_region_evidence_id: u64,
    rect_client_px: MtgoRectPxV1,
}

/// One gesture stage resolved against a complete target set on the exact
/// required visible frame. Target rectangles remain private. This value has
/// no input conversion and does not prove that a classifier label matches its
/// pixels.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoDuelGestureStageBindingV1;
/// fn cannot_act(value: &CheckedUntrustedMtgoDuelGestureStageBindingV1) {
///     let _ = value.target_rectangles();
///     let _ = value.send_input();
/// }
/// ```
pub struct CheckedUntrustedMtgoDuelGestureStageBindingV1 {
    commitments: MtgoDuelGestureStageBindingCommitmentsV1,
    _targets: Vec<PrivateVisibleGestureTargetV1>,
}

impl CheckedUntrustedMtgoDuelGestureStageBindingV1 {
    pub fn commitments_v1(&self) -> MtgoDuelGestureStageBindingCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_live_input(&self) -> bool {
        false
    }

    pub fn permits_event_entry(&self) -> bool {
        false
    }
}

pub fn validate_profile_bound_duel_gesture_plan_v1(
    resolved: &CheckedUntrustedMtgoProfileBoundResolvedActionControlV1,
    plan: MtgoDuelGesturePlanV1,
) -> Result<CheckedUntrustedMtgoDuelGesturePlanV1, MtgoContractErrorV1> {
    if plan.schema_version != MTGO_DUEL_GESTURE_PLAN_SCHEMA_V1 {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_schema_mismatch",
            plan.schema_version.to_string(),
        ));
    }
    if plan.profile_bound_resolution_commitment_sha256
        != resolved.profile_bound_resolution_commitment_sha256()
        || plan.decision_commitment_sha256 != resolved.decision_commitment_sha256()
        || plan.selection_commitment_sha256 != resolved.selection_commitment_sha256()
    {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_source_mismatch",
            "gesture plan must bind the exact selected visible control",
        ));
    }
    if plan.frame_id != resolved.frame_id() || plan.frame_sequence != resolved.frame_sequence() {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_frame_mismatch",
            "gesture plan must begin at the selected control frame",
        ));
    }
    let selected_family = duel_action_family_v1(resolved.selected_semantic());
    if plan.selected_action_family != selected_family {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_family_mismatch",
            "declared family differs from the selected semantic",
        ));
    }
    if !plan.stage_set_complete {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_stage_set_incomplete",
            "every input stage must be declared before use",
        ));
    }
    validate_duel_gesture_shape_v1(resolved.selected_semantic(), &plan.stages)?;

    let encoded = serde_json::to_vec(&plan).map_err(|error| {
        MtgoContractErrorV1::new("duel_gesture_serialization_failed", error.to_string())
    })?;
    let mut hasher = Sha256::new();
    hasher.update(DUEL_GESTURE_PLAN_COMMITMENT_DOMAIN_V1);
    hasher.update((encoded.len() as u64).to_le_bytes());
    hasher.update(encoded);
    hasher.update(b"checked_untrusted_no_coordinates_no_input_or_event_entry_authority");
    Ok(CheckedUntrustedMtgoDuelGesturePlanV1 {
        plan,
        selected_semantic: resolved.selected_semantic().clone(),
        plan_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

pub fn bind_visible_duel_gesture_stage_v1(
    plan: &CheckedUntrustedMtgoDuelGesturePlanV1,
    decision: &ValidatedMtgoObservedDecisionV1,
    target_set: MtgoVisibleDuelGestureTargetSetV1,
) -> Result<CheckedUntrustedMtgoDuelGestureStageBindingV1, MtgoContractErrorV1> {
    bind_visible_duel_gesture_stage_inner_v1(
        plan,
        decision,
        target_set,
        MtgoDuelGestureStageBindingModeV1::DeclaredFrame,
    )
}

/// Rechecks stage zero of an already selected gesture plan against a distinct,
/// strictly newer visible frame on which the same selected semantic remains
/// legal. This is a coordinate-private pre-input freshness check only. The
/// caller must separately prove that the newer decision belongs to the same
/// process and window incarnation and that its visible payload is unchanged.
pub fn recheck_visible_duel_gesture_source_stage_v1(
    plan: &CheckedUntrustedMtgoDuelGesturePlanV1,
    decision: &ValidatedMtgoObservedDecisionV1,
    target_set: MtgoVisibleDuelGestureTargetSetV1,
) -> Result<CheckedUntrustedMtgoDuelGestureStageBindingV1, MtgoContractErrorV1> {
    bind_visible_duel_gesture_stage_inner_v1(
        plan,
        decision,
        target_set,
        MtgoDuelGestureStageBindingModeV1::FreshSourceRecheck,
    )
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum MtgoDuelGestureStageBindingModeV1 {
    DeclaredFrame,
    FreshSourceRecheck,
}

fn bind_visible_duel_gesture_stage_inner_v1(
    plan: &CheckedUntrustedMtgoDuelGesturePlanV1,
    decision: &ValidatedMtgoObservedDecisionV1,
    target_set: MtgoVisibleDuelGestureTargetSetV1,
    binding_mode: MtgoDuelGestureStageBindingModeV1,
) -> Result<CheckedUntrustedMtgoDuelGestureStageBindingV1, MtgoContractErrorV1> {
    if target_set.schema_version != MTGO_DUEL_GESTURE_TARGET_SET_SCHEMA_V1 {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_target_schema_mismatch",
            target_set.schema_version.to_string(),
        ));
    }
    if target_set.gesture_plan_commitment_sha256 != plan.plan_commitment_sha256
        || target_set.decision_commitment_sha256 != decision.decision_commitment_sha256()
    {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_target_source_mismatch",
            "target set must bind the exact gesture plan and visible decision",
        ));
    }
    let Some(stage) = plan.plan.stages.get(usize::from(target_set.stage_index)) else {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_target_stage_invalid",
            target_set.stage_index.to_string(),
        ));
    };
    if stage.stage_index != target_set.stage_index {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_target_stage_invalid",
            target_set.stage_index.to_string(),
        ));
    }
    if target_set.frame_id != decision.frame_id()
        || target_set.frame_sequence != decision.frame_sequence()
    {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_target_frame_mismatch",
            "target set must bind the supplied visible frame",
        ));
    }
    match binding_mode {
        MtgoDuelGestureStageBindingModeV1::DeclaredFrame => match stage.frame_binding {
            MtgoDuelGestureFrameBindingV1::SourceDecisionFrame => {
                if target_set.frame_id != plan.plan.frame_id
                    || target_set.frame_sequence != plan.plan.frame_sequence
                    || target_set.decision_commitment_sha256 != plan.plan.decision_commitment_sha256
                {
                    return Err(MtgoContractErrorV1::new(
                        "duel_gesture_source_stage_frame_mismatch",
                        "source stage must use the exact selected decision frame",
                    ));
                }
            }
            MtgoDuelGestureFrameBindingV1::StrictlyNewerVisibleContinuation => {
                if target_set.frame_sequence <= plan.plan.frame_sequence
                    || target_set.frame_id == plan.plan.frame_id
                {
                    return Err(MtgoContractErrorV1::new(
                        "duel_gesture_continuation_not_newer",
                        "continuation stage requires a distinct strictly newer visible frame",
                    ));
                }
            }
        },
        MtgoDuelGestureStageBindingModeV1::FreshSourceRecheck => {
            if stage.stage_index != 0
                || stage.frame_binding != MtgoDuelGestureFrameBindingV1::SourceDecisionFrame
            {
                return Err(MtgoContractErrorV1::new(
                    "duel_gesture_source_recheck_stage_invalid",
                    "fresh source recheck accepts only declared source stage zero",
                ));
            }
            if target_set.frame_sequence <= plan.plan.frame_sequence
                || target_set.frame_id == plan.plan.frame_id
            {
                return Err(MtgoContractErrorV1::new(
                    "duel_gesture_source_recheck_not_newer",
                    "source-stage recheck requires a distinct strictly newer visible frame",
                ));
            }
        }
    }
    if !decision.legal_actions().contains(&plan.selected_semantic) {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_semantic_no_longer_legal",
            "the selected semantic must remain legal on the bound stage frame",
        ));
    }
    if !target_set.candidate_set_complete
        || target_set.targets.is_empty()
        || target_set.targets.len() > MAX_DUEL_GESTURE_TARGETS_V1
    {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_target_set_incomplete",
            target_set.targets.len().to_string(),
        ));
    }

    let mut target_ids = HashSet::new();
    let mut evidence_ids = HashSet::new();
    let mut checked_targets = Vec::with_capacity(target_set.targets.len());
    for target in &target_set.targets {
        validate_target_identifier_v1(&target.target_id)?;
        if !target_ids.insert(target.target_id.as_str()) {
            return Err(MtgoContractErrorV1::new(
                "duel_gesture_target_id_duplicate",
                &target.target_id,
            ));
        }
        if !evidence_ids.insert(target.frame_region_evidence_id) {
            return Err(MtgoContractErrorV1::new(
                "duel_gesture_target_evidence_duplicate",
                target.frame_region_evidence_id.to_string(),
            ));
        }
        if !target.visibly_enabled
            || target.confidence_bps < MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1
            || target.confidence_bps > 10_000
        {
            return Err(MtgoContractErrorV1::new(
                "duel_gesture_target_not_reliable",
                &target.target_id,
            ));
        }
        if let MtgoDuelGestureTargetRoleV1::SemanticObject { object } = &target.role {
            if !decision
                .object_bindings()
                .iter()
                .any(|binding| &binding.kernel_ref == object)
            {
                return Err(MtgoContractErrorV1::new(
                    "duel_gesture_target_object_unbound",
                    &target.target_id,
                ));
            }
        }
        let rect = exact_current_frame_region_v1(
            decision,
            target.frame_region_evidence_id,
            "duel_gesture_target_evidence_invalid",
        )?;
        checked_targets.push(PrivateVisibleGestureTargetV1 {
            target_id: target.target_id.clone(),
            role: target.role.clone(),
            frame_region_evidence_id: target.frame_region_evidence_id,
            rect_client_px: rect.clone(),
        });
    }

    let required_roles = required_duel_gesture_target_roles_v1(&stage.primitive);
    let mut selected_targets = Vec::with_capacity(required_roles.len());
    for required in required_roles {
        let matches: Vec<&PrivateVisibleGestureTargetV1> = checked_targets
            .iter()
            .filter(|target| target.role == required)
            .collect();
        if matches.len() != 1 {
            return Err(MtgoContractErrorV1::new(
                "duel_gesture_target_match_count",
                matches.len().to_string(),
            ));
        }
        selected_targets.push(PrivateVisibleGestureTargetV1 {
            target_id: matches[0].target_id.clone(),
            role: matches[0].role.clone(),
            frame_region_evidence_id: matches[0].frame_region_evidence_id,
            rect_client_px: matches[0].rect_client_px.clone(),
        });
    }
    if selected_targets.len() == 2
        && rects_intersect_v1(
            &selected_targets[0].rect_client_px,
            &selected_targets[1].rect_client_px,
        )
    {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_drag_targets_overlap",
            "drag source and destination must be visibly distinct",
        ));
    }

    #[derive(Serialize)]
    struct StageBindingRecordV1<'a> {
        binding_mode: &'a MtgoDuelGestureStageBindingModeV1,
        target_set: &'a MtgoVisibleDuelGestureTargetSetV1,
        stage: &'a MtgoDuelGestureStageV1,
        selected_targets: &'a [PrivateVisibleGestureTargetV1],
    }
    let encoded = serde_json::to_vec(&StageBindingRecordV1 {
        binding_mode: &binding_mode,
        target_set: &target_set,
        stage,
        selected_targets: &selected_targets,
    })
    .map_err(|error| {
        MtgoContractErrorV1::new(
            "duel_gesture_target_serialization_failed",
            error.to_string(),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(DUEL_GESTURE_STAGE_BINDING_COMMITMENT_DOMAIN_V1);
    hasher.update((encoded.len() as u64).to_le_bytes());
    hasher.update(encoded);
    hasher.update(b"checked_untrusted_private_rectangles_no_input_or_event_entry_authority");
    Ok(CheckedUntrustedMtgoDuelGestureStageBindingV1 {
        commitments: MtgoDuelGestureStageBindingCommitmentsV1 {
            gesture_plan_commitment_sha256: plan.plan_commitment_sha256.clone(),
            decision_commitment_sha256: target_set.decision_commitment_sha256,
            stage_index: target_set.stage_index,
            frame_id: target_set.frame_id,
            frame_sequence: target_set.frame_sequence,
            target_count: selected_targets.len() as u16,
            binding_commitment_sha256: format!("{:x}", hasher.finalize()),
        },
        _targets: selected_targets,
    })
}

/// Validates only the semantic shape of a gesture. This is useful for corpus
/// tooling and tests, but it grants no authority and binds no pixels.
pub fn validate_duel_gesture_shape_v1(
    semantic: &ActionSemanticV1,
    stages: &[MtgoDuelGestureStageV1],
) -> Result<(), MtgoContractErrorV1> {
    validate_stage_envelope_v1(stages)?;
    match semantic {
        ActionSemanticV1::Pass { .. } => {
            require_direct_primary_v1(stages, &[MtgoDuelPrimaryActivationV1::SingleLeftClick])
        }
        ActionSemanticV1::PlayLand { .. }
        | ActionSemanticV1::CastSpell { .. }
        | ActionSemanticV1::PlotSpell { .. } => {
            if require_direct_primary_v1(
                stages,
                &[
                    MtgoDuelPrimaryActivationV1::SingleLeftClick,
                    MtgoDuelPrimaryActivationV1::DoubleLeftClick,
                ],
            )
            .is_ok()
                || matches!(
                    stages,
                    [MtgoDuelGestureStageV1 {
                        primitive: MtgoDuelGesturePrimitiveV1::DragPrimaryToCalibratedPlayArea,
                        ..
                    }]
                )
            {
                Ok(())
            } else {
                Err(shape_error_v1(
                    "play or cast gesture must be direct activation or drag",
                ))
            }
        }
        ActionSemanticV1::ActivateManaAbility { .. } | ActionSemanticV1::ActivateAbility { .. } => {
            if require_direct_primary_v1(
                stages,
                &[
                    MtgoDuelPrimaryActivationV1::SingleLeftClick,
                    MtgoDuelPrimaryActivationV1::DoubleLeftClick,
                ],
            )
            .is_ok()
                || is_context_menu_v1(stages)
            {
                Ok(())
            } else {
                Err(shape_error_v1(
                    "ability gesture must be direct activation or a fresh-frame context-menu choice",
                ))
            }
        }
        ActionSemanticV1::ChooseTarget { .. }
        | ActionSemanticV1::ChooseCostTarget { .. }
        | ActionSemanticV1::ChooseCastMode { .. }
        | ActionSemanticV1::ChooseKicker { .. }
        | ActionSemanticV1::ChooseSpellMode { .. }
        | ActionSemanticV1::ChooseEffectOption { .. }
        | ActionSemanticV1::ChooseEffectTarget { .. }
        | ActionSemanticV1::FinishEffectSelection { .. }
        | ActionSemanticV1::ChooseEffectColor { .. }
        | ActionSemanticV1::ChooseEffectNumber { .. }
        | ActionSemanticV1::ChooseEffectBoolean { .. }
        | ActionSemanticV1::FinishTargetSelection { .. }
        | ActionSemanticV1::ChooseOptionalCostUse { .. }
        | ActionSemanticV1::ChooseOptionalCostWhich { .. }
        | ActionSemanticV1::ChooseSpellCopyPayment { .. }
        | ActionSemanticV1::ChooseSpellCopyRetarget { .. }
        | ActionSemanticV1::ChooseMadnessCast { .. } => {
            require_direct_primary_v1(stages, &[MtgoDuelPrimaryActivationV1::SingleLeftClick])
        }
        ActionSemanticV1::Discard { cards, .. } => {
            require_object_selection_then_submit_v1(stages, cards)
        }
        ActionSemanticV1::DeclareAttackers { attackers, .. } => {
            require_object_selection_then_submit_v1(stages, attackers)
        }
        ActionSemanticV1::DeclareBlockersForAttacker { blockers, .. } => {
            require_object_selection_then_submit_v1(stages, blockers)
        }
        ActionSemanticV1::ChooseAttackerInclusion { .. }
        | ActionSemanticV1::ChooseBlockerInclusion { .. } => {
            require_direct_primary_v1(stages, &[MtgoDuelPrimaryActivationV1::SingleLeftClick])
        }
        ActionSemanticV1::OrderTriggers {
            pending_sources,
            order,
            ..
        } => require_trigger_order_v1(stages, pending_sources, order),
        ActionSemanticV1::Ambiguous { .. } => Err(MtgoContractErrorV1::new(
            "duel_gesture_ambiguous_semantic",
            "ambiguous actions cannot produce gestures",
        )),
    }
}

fn validate_stage_envelope_v1(
    stages: &[MtgoDuelGestureStageV1],
) -> Result<(), MtgoContractErrorV1> {
    if stages.is_empty() || stages.len() > MAX_DUEL_GESTURE_STAGES_V1 {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_stage_count_invalid",
            stages.len().to_string(),
        ));
    }
    for (index, stage) in stages.iter().enumerate() {
        if usize::from(stage.stage_index) != index {
            return Err(MtgoContractErrorV1::new(
                "duel_gesture_stage_index_invalid",
                stage.stage_index.to_string(),
            ));
        }
        let expected_binding = if index == 0 {
            MtgoDuelGestureFrameBindingV1::SourceDecisionFrame
        } else {
            MtgoDuelGestureFrameBindingV1::StrictlyNewerVisibleContinuation
        };
        if stage.frame_binding != expected_binding {
            return Err(MtgoContractErrorV1::new(
                "duel_gesture_frame_binding_invalid",
                index.to_string(),
            ));
        }
        let expected_effect = if index + 1 == stages.len() {
            MtgoDuelGestureExpectedEffectV1::CompleteSelectedSemantic
        } else {
            MtgoDuelGestureExpectedEffectV1::ContinueSelectedSemantic
        };
        if stage.expected_effect != expected_effect {
            return Err(MtgoContractErrorV1::new(
                "duel_gesture_expected_effect_invalid",
                index.to_string(),
            ));
        }
    }
    Ok(())
}

fn require_direct_primary_v1(
    stages: &[MtgoDuelGestureStageV1],
    allowed: &[MtgoDuelPrimaryActivationV1],
) -> Result<(), MtgoContractErrorV1> {
    match stages {
        [MtgoDuelGestureStageV1 {
            primitive: MtgoDuelGesturePrimitiveV1::ActivatePrimary { activation },
            ..
        }] if allowed.contains(activation) => Ok(()),
        _ => Err(shape_error_v1("direct primary gesture shape is invalid")),
    }
}

fn is_context_menu_v1(stages: &[MtgoDuelGestureStageV1]) -> bool {
    matches!(
        stages,
        [
            MtgoDuelGestureStageV1 {
                primitive: MtgoDuelGesturePrimitiveV1::ActivatePrimary {
                    activation: MtgoDuelPrimaryActivationV1::RightClick
                },
                ..
            },
            MtgoDuelGestureStageV1 {
                primitive: MtgoDuelGesturePrimitiveV1::ActivateSemanticMenuChoice {
                    activation: MtgoDuelPrimaryActivationV1::SingleLeftClick
                },
                ..
            }
        ]
    )
}

fn require_object_selection_then_submit_v1(
    stages: &[MtgoDuelGestureStageV1],
    expected_objects: &[CardStableRefV1],
) -> Result<(), MtgoContractErrorV1> {
    require_unique_objects_v1(expected_objects)?;
    if stages.len() != expected_objects.len() + 1
        || !matches!(
            stages.last().map(|stage| &stage.primitive),
            Some(MtgoDuelGesturePrimitiveV1::Submit)
        )
    {
        return Err(shape_error_v1(
            "compound object gesture must select every semantic object then submit",
        ));
    }
    for (stage, expected) in stages.iter().zip(expected_objects) {
        match &stage.primitive {
            MtgoDuelGesturePrimitiveV1::SelectObject { object } if object == expected => {}
            _ => {
                return Err(shape_error_v1(
                    "object selection order differs from the selected semantic",
                ))
            }
        }
    }
    Ok(())
}

fn require_trigger_order_v1(
    stages: &[MtgoDuelGestureStageV1],
    pending_sources: &[CardStableRefV1],
    order: &[usize],
) -> Result<(), MtgoContractErrorV1> {
    require_unique_objects_v1(pending_sources)?;
    if pending_sources.is_empty()
        || order.len() != pending_sources.len()
        || stages.len() != pending_sources.len() + 1
        || !matches!(
            stages.last().map(|stage| &stage.primitive),
            Some(MtgoDuelGesturePrimitiveV1::Submit)
        )
    {
        return Err(shape_error_v1(
            "trigger-order gesture cardinality is invalid",
        ));
    }
    let order_set: HashSet<usize> = order.iter().copied().collect();
    if order_set.len() != order.len() || order_set.iter().any(|index| *index >= order.len()) {
        return Err(shape_error_v1(
            "trigger order is not a complete permutation",
        ));
    }
    let click_shape = stages[..order.len()]
        .iter()
        .zip(order)
        .all(|(stage, source_index)| {
            matches!(
                &stage.primitive,
                MtgoDuelGesturePrimitiveV1::SelectObject { object }
                    if object == &pending_sources[*source_index]
            )
        });
    let drag_shape = stages[..order.len()]
        .iter()
        .zip(order)
        .enumerate()
        .all(|(slot_index, (stage, source_index))| {
            matches!(
                &stage.primitive,
                MtgoDuelGesturePrimitiveV1::DragObjectToOrderSlot { object, slot_index: actual_slot }
                    if object == &pending_sources[*source_index]
                        && usize::from(*actual_slot) == slot_index
            )
        });
    if click_shape || drag_shape {
        Ok(())
    } else {
        Err(shape_error_v1(
            "trigger gesture does not implement the selected semantic order",
        ))
    }
}

fn require_unique_objects_v1(objects: &[CardStableRefV1]) -> Result<(), MtgoContractErrorV1> {
    let unique: HashSet<&CardStableRefV1> = objects.iter().collect();
    if unique.len() != objects.len() {
        return Err(shape_error_v1("semantic object list contains duplicates"));
    }
    Ok(())
}

fn shape_error_v1(detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new("duel_gesture_shape_invalid", detail)
}

pub fn required_duel_gesture_target_roles_v1(
    primitive: &MtgoDuelGesturePrimitiveV1,
) -> Vec<MtgoDuelGestureTargetRoleV1> {
    match primitive {
        MtgoDuelGesturePrimitiveV1::ActivatePrimary { .. } => {
            vec![MtgoDuelGestureTargetRoleV1::PrimarySemanticControl]
        }
        MtgoDuelGesturePrimitiveV1::ActivateSemanticMenuChoice { .. } => {
            vec![MtgoDuelGestureTargetRoleV1::SemanticMenuChoice]
        }
        MtgoDuelGesturePrimitiveV1::DragPrimaryToCalibratedPlayArea => vec![
            MtgoDuelGestureTargetRoleV1::PrimarySemanticControl,
            MtgoDuelGestureTargetRoleV1::CalibratedPlayArea,
        ],
        MtgoDuelGesturePrimitiveV1::SelectObject { object } => {
            vec![MtgoDuelGestureTargetRoleV1::SemanticObject {
                object: object.clone(),
            }]
        }
        MtgoDuelGesturePrimitiveV1::DragObjectToOrderSlot { object, slot_index } => vec![
            MtgoDuelGestureTargetRoleV1::SemanticObject {
                object: object.clone(),
            },
            MtgoDuelGestureTargetRoleV1::OrderSlot {
                slot_index: *slot_index,
            },
        ],
        MtgoDuelGesturePrimitiveV1::Submit => {
            vec![MtgoDuelGestureTargetRoleV1::SubmitControl]
        }
    }
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

fn validate_target_identifier_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(MtgoContractErrorV1::new(
            "duel_gesture_target_id_invalid",
            value,
        ));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile_bound_resolved_action_control_for_test_v1;
    use mtg_kernel::rl::PlayerSeatV1;
    use mtg_kernel::state::Zone;

    fn stage_v1(
        index: u16,
        primitive: MtgoDuelGesturePrimitiveV1,
        count: usize,
    ) -> MtgoDuelGestureStageV1 {
        MtgoDuelGestureStageV1 {
            stage_index: index,
            frame_binding: if index == 0 {
                MtgoDuelGestureFrameBindingV1::SourceDecisionFrame
            } else {
                MtgoDuelGestureFrameBindingV1::StrictlyNewerVisibleContinuation
            },
            primitive,
            expected_effect: if usize::from(index) + 1 == count {
                MtgoDuelGestureExpectedEffectV1::CompleteSelectedSemantic
            } else {
                MtgoDuelGestureExpectedEffectV1::ContinueSelectedSemantic
            },
        }
    }

    fn card_v1(arena_id: u32) -> CardStableRefV1 {
        CardStableRefV1 {
            arena_id,
            card_db_id: arena_id as u16,
            owner: PlayerSeatV1::P0,
            controller: PlayerSeatV1::P0,
            zone: Zone::Battlefield,
            zone_change_count: 0,
        }
    }

    #[test]
    fn profile_bound_play_land_plan_is_committed_without_authority() {
        let resolved = profile_bound_resolved_action_control_for_test_v1();
        let source = resolved
            .profile_bound_resolution_commitment_sha256()
            .to_owned();
        let decision = resolved.decision_commitment_sha256().to_owned();
        let selection = resolved.selection_commitment_sha256().to_owned();
        let frame_id = resolved.frame_id();
        let frame_sequence = resolved.frame_sequence();
        let plan = MtgoDuelGesturePlanV1 {
            schema_version: MTGO_DUEL_GESTURE_PLAN_SCHEMA_V1,
            profile_bound_resolution_commitment_sha256: source,
            decision_commitment_sha256: decision,
            selection_commitment_sha256: selection,
            frame_id,
            frame_sequence,
            selected_action_family: MtgoDuelActionFamilyV1::PlayLand,
            stage_set_complete: true,
            stages: vec![stage_v1(
                0,
                MtgoDuelGesturePrimitiveV1::ActivatePrimary {
                    activation: MtgoDuelPrimaryActivationV1::DoubleLeftClick,
                },
                1,
            )],
        };
        let checked = validate_profile_bound_duel_gesture_plan_v1(&resolved, plan).unwrap();
        assert_eq!(checked.commitments_v1().stage_count, 1);
        assert_eq!(checked.commitments_v1().plan_commitment_sha256.len(), 64);
        assert!(!checked.safe_for_live_input());
        assert!(!checked.permits_event_entry());
    }

    #[test]
    fn every_duel_action_family_has_a_bounded_representative_shape() {
        let first = card_v1(1);
        let second = card_v1(2);
        let direct = |activation| {
            vec![stage_v1(
                0,
                MtgoDuelGesturePrimitiveV1::ActivatePrimary { activation },
                1,
            )]
        };
        let representatives = vec![
            (
                ActionSemanticV1::Pass {
                    actor: PlayerSeatV1::P0,
                },
                direct(MtgoDuelPrimaryActivationV1::SingleLeftClick),
            ),
            (
                ActionSemanticV1::PlayLand {
                    actor: PlayerSeatV1::P0,
                    source: first.clone(),
                },
                direct(MtgoDuelPrimaryActivationV1::DoubleLeftClick),
            ),
            (
                ActionSemanticV1::CastSpell {
                    actor: PlayerSeatV1::P0,
                    source: first.clone(),
                },
                vec![stage_v1(
                    0,
                    MtgoDuelGesturePrimitiveV1::DragPrimaryToCalibratedPlayArea,
                    1,
                )],
            ),
            (
                ActionSemanticV1::ActivateManaAbility {
                    actor: PlayerSeatV1::P0,
                    source: first.clone(),
                    mana_choice: None,
                },
                direct(MtgoDuelPrimaryActivationV1::SingleLeftClick),
            ),
            (
                ActionSemanticV1::ActivateAbility {
                    actor: PlayerSeatV1::P0,
                    source: first.clone(),
                    ability_index: 0,
                },
                vec![
                    stage_v1(
                        0,
                        MtgoDuelGesturePrimitiveV1::ActivatePrimary {
                            activation: MtgoDuelPrimaryActivationV1::RightClick,
                        },
                        2,
                    ),
                    stage_v1(
                        1,
                        MtgoDuelGesturePrimitiveV1::ActivateSemanticMenuChoice {
                            activation: MtgoDuelPrimaryActivationV1::SingleLeftClick,
                        },
                        2,
                    ),
                ],
            ),
            (
                ActionSemanticV1::FinishTargetSelection {
                    actor: PlayerSeatV1::P0,
                    source: first.clone(),
                    selected_count: 1,
                },
                direct(MtgoDuelPrimaryActivationV1::SingleLeftClick),
            ),
            (
                ActionSemanticV1::ChooseOptionalCostUse {
                    actor: PlayerSeatV1::P0,
                    use_cost: true,
                },
                direct(MtgoDuelPrimaryActivationV1::SingleLeftClick),
            ),
            (
                ActionSemanticV1::FinishEffectSelection {
                    actor: PlayerSeatV1::P0,
                    source: first.clone(),
                    selected_count: 1,
                },
                direct(MtgoDuelPrimaryActivationV1::SingleLeftClick),
            ),
            (
                ActionSemanticV1::Discard {
                    actor: PlayerSeatV1::P0,
                    cards: vec![first.clone(), second.clone()],
                },
                vec![
                    stage_v1(
                        0,
                        MtgoDuelGesturePrimitiveV1::SelectObject {
                            object: first.clone(),
                        },
                        3,
                    ),
                    stage_v1(
                        1,
                        MtgoDuelGesturePrimitiveV1::SelectObject {
                            object: second.clone(),
                        },
                        3,
                    ),
                    stage_v1(2, MtgoDuelGesturePrimitiveV1::Submit, 3),
                ],
            ),
            (
                ActionSemanticV1::DeclareAttackers {
                    actor: PlayerSeatV1::P0,
                    attackers: vec![first.clone()],
                },
                vec![
                    stage_v1(
                        0,
                        MtgoDuelGesturePrimitiveV1::SelectObject {
                            object: first.clone(),
                        },
                        2,
                    ),
                    stage_v1(1, MtgoDuelGesturePrimitiveV1::Submit, 2),
                ],
            ),
            (
                ActionSemanticV1::OrderTriggers {
                    actor: PlayerSeatV1::P0,
                    pending_sources: vec![first.clone(), second.clone()],
                    order: vec![1, 0],
                },
                vec![
                    stage_v1(
                        0,
                        MtgoDuelGesturePrimitiveV1::DragObjectToOrderSlot {
                            object: second,
                            slot_index: 0,
                        },
                        3,
                    ),
                    stage_v1(
                        1,
                        MtgoDuelGesturePrimitiveV1::DragObjectToOrderSlot {
                            object: first,
                            slot_index: 1,
                        },
                        3,
                    ),
                    stage_v1(2, MtgoDuelGesturePrimitiveV1::Submit, 3),
                ],
            ),
        ];

        let mut families = HashSet::new();
        for (semantic, stages) in representatives {
            validate_duel_gesture_shape_v1(&semantic, &stages).unwrap();
            families.insert(duel_action_family_v1(&semantic));
        }
        assert_eq!(families.len(), 11);
    }

    #[test]
    fn stale_stage_relations_and_semantic_mismatches_fail_closed() {
        let action = ActionSemanticV1::Discard {
            actor: PlayerSeatV1::P0,
            cards: vec![card_v1(1)],
        };
        let mut stages = vec![
            stage_v1(
                0,
                MtgoDuelGesturePrimitiveV1::SelectObject { object: card_v1(1) },
                2,
            ),
            stage_v1(1, MtgoDuelGesturePrimitiveV1::Submit, 2),
        ];
        stages[1].frame_binding = MtgoDuelGestureFrameBindingV1::SourceDecisionFrame;
        assert_eq!(
            validate_duel_gesture_shape_v1(&action, &stages)
                .unwrap_err()
                .code(),
            "duel_gesture_frame_binding_invalid"
        );

        stages[1].frame_binding = MtgoDuelGestureFrameBindingV1::StrictlyNewerVisibleContinuation;
        stages[0].primitive = MtgoDuelGesturePrimitiveV1::SelectObject { object: card_v1(2) };
        assert_eq!(
            validate_duel_gesture_shape_v1(&action, &stages)
                .unwrap_err()
                .code(),
            "duel_gesture_shape_invalid"
        );
    }

    #[test]
    fn visible_stage_binding_requires_one_exact_current_frame_target() {
        let resolved = profile_bound_resolved_action_control_for_test_v1();
        let decision = resolved.validated_decision_v1();
        let plan = MtgoDuelGesturePlanV1 {
            schema_version: MTGO_DUEL_GESTURE_PLAN_SCHEMA_V1,
            profile_bound_resolution_commitment_sha256: resolved
                .profile_bound_resolution_commitment_sha256()
                .to_owned(),
            decision_commitment_sha256: resolved.decision_commitment_sha256().to_owned(),
            selection_commitment_sha256: resolved.selection_commitment_sha256().to_owned(),
            frame_id: resolved.frame_id(),
            frame_sequence: resolved.frame_sequence(),
            selected_action_family: MtgoDuelActionFamilyV1::PlayLand,
            stage_set_complete: true,
            stages: vec![stage_v1(
                0,
                MtgoDuelGesturePrimitiveV1::ActivatePrimary {
                    activation: MtgoDuelPrimaryActivationV1::DoubleLeftClick,
                },
                1,
            )],
        };
        let checked_plan = validate_profile_bound_duel_gesture_plan_v1(&resolved, plan).unwrap();
        let target = MtgoVisibleDuelGestureTargetCandidateV1 {
            target_id: "selected-hand-land".to_owned(),
            role: MtgoDuelGestureTargetRoleV1::PrimarySemanticControl,
            frame_region_evidence_id: 50,
            confidence_bps: 10_000,
            visibly_enabled: true,
        };
        let target_set = MtgoVisibleDuelGestureTargetSetV1 {
            schema_version: MTGO_DUEL_GESTURE_TARGET_SET_SCHEMA_V1,
            gesture_plan_commitment_sha256: checked_plan.commitments_v1().plan_commitment_sha256,
            decision_commitment_sha256: decision.decision_commitment_sha256().to_owned(),
            frame_id: decision.frame_id(),
            frame_sequence: decision.frame_sequence(),
            stage_index: 0,
            candidate_set_complete: true,
            targets: vec![target.clone()],
        };
        let bound = bind_visible_duel_gesture_stage_v1(&checked_plan, decision, target_set.clone())
            .unwrap();
        assert_eq!(bound.commitments_v1().target_count, 1);
        assert_eq!(bound.commitments_v1().binding_commitment_sha256.len(), 64);
        assert!(!bound.safe_for_live_input());
        assert_eq!(
            recheck_visible_duel_gesture_source_stage_v1(
                &checked_plan,
                decision,
                target_set.clone(),
            )
            .err()
            .unwrap()
            .code(),
            "duel_gesture_source_recheck_not_newer"
        );

        let mut stale = target_set.clone();
        stale.frame_sequence += 1;
        assert_eq!(
            bind_visible_duel_gesture_stage_v1(&checked_plan, decision, stale)
                .err()
                .unwrap()
                .code(),
            "duel_gesture_target_frame_mismatch"
        );

        let mut ambiguous = target_set;
        ambiguous
            .targets
            .push(MtgoVisibleDuelGestureTargetCandidateV1 {
                target_id: "wrong-second-primary".to_owned(),
                role: MtgoDuelGestureTargetRoleV1::PrimarySemanticControl,
                frame_region_evidence_id: 40,
                confidence_bps: 10_000,
                visibly_enabled: true,
            });
        assert_eq!(
            bind_visible_duel_gesture_stage_v1(&checked_plan, decision, ambiguous)
                .err()
                .unwrap()
                .code(),
            "duel_gesture_target_match_count"
        );
    }
}
