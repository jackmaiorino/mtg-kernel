use crate::{
    duel_action_family_v1, CheckedUntrustedMtgoProfileBoundResolvedActionControlV1,
    MtgoContractErrorV1, MtgoDuelActionFamilyV1,
};
use mtg_kernel::rl::{ActionSemanticV1, CardStableRefV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_DUEL_GESTURE_PLAN_SCHEMA_V1: u32 = 1;

const DUEL_GESTURE_PLAN_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-duel-gesture-plan-v1";
const MAX_DUEL_GESTURE_STAGES_V1: usize = 32;

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
        plan_commitment_sha256: format!("{:x}", hasher.finalize()),
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
}
