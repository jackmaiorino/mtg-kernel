use crate::{
    MtgoContractErrorV1, MtgoDuelActionFamilyV1, MtgoDuelPrimaryActivationV1,
    MtgoPlayerRelativeRoleV1, MtgoPlayerVisibleDuelActionV1, MtgoPlayerVisibleObjectRefV1,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MTGO_PLAYER_VISIBLE_DUEL_GESTURE_PLAN_SCHEMA_V1: u32 = 1;

const MAX_PLAYER_VISIBLE_DUEL_GESTURE_PRIMITIVES_V1: usize = 32;

/// One coordinate-free UI operation expressed only with values already
/// present in the seated player's visible legal action.
///
/// Visible object ordinals are scoped to one player-visible decision input.
/// They are not MTGO IDs, kernel object references, card-database IDs, or
/// persistent identities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "primitive_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoPlayerVisibleDuelGesturePrimitiveV1 {
    ActivatePrimary {
        activation: MtgoDuelPrimaryActivationV1,
    },
    ActivateSemanticMenuChoice {
        activation: MtgoDuelPrimaryActivationV1,
    },
    DragPrimaryToCalibratedPlayArea,
    SelectVisibleObject {
        object: MtgoPlayerVisibleObjectRefV1,
    },
    DragVisibleObjectToOrderSlot {
        object: MtgoPlayerVisibleObjectRefV1,
        slot_index: u16,
    },
    Submit,
}

/// One visual role required to locate a player-visible gesture primitive.
/// Object roles carry only the transient ordinal from the same visible
/// decision input. This type contains no rectangle, evidence identity, or
/// adapter lineage.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "target_role", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoPlayerVisibleDuelGestureTargetRoleV1 {
    PrimarySemanticControl,
    SemanticMenuChoice,
    VisibleObject {
        object: MtgoPlayerVisibleObjectRefV1,
    },
    CalibratedPlayArea,
    OrderSlot {
        slot_index: u16,
    },
    SubmitControl,
}

/// Player-visible action plus its complete coordinate-free UI operation
/// sequence. It contains no capture, frame, process, control, source,
/// authorization, or internal card identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleDuelGesturePlanV1 {
    pub schema_version: u32,
    pub selected_action: MtgoPlayerVisibleDuelActionV1,
    pub primitive_set_complete: bool,
    pub primitives: Vec<MtgoPlayerVisibleDuelGesturePrimitiveV1>,
}

/// Structurally checked player-visible gesture plan. This value can be passed
/// to a later private visible-control binder, but it has no coordinates or
/// input authority by itself.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoPlayerVisibleDuelGesturePlanV1;
/// fn cannot_read_private_adapter_state(
///     value: &CheckedUntrustedMtgoPlayerVisibleDuelGesturePlanV1,
/// ) {
///     let _ = value.frame_id();
///     let _ = value.control_id();
///     let _ = value.kernel_object_ref();
///     let _ = value.input_command();
/// }
/// ```
pub struct CheckedUntrustedMtgoPlayerVisibleDuelGesturePlanV1 {
    plan: MtgoPlayerVisibleDuelGesturePlanV1,
    action_family: MtgoDuelActionFamilyV1,
}

impl CheckedUntrustedMtgoPlayerVisibleDuelGesturePlanV1 {
    pub fn selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.plan.selected_action
    }

    pub fn primitives_v1(&self) -> &[MtgoPlayerVisibleDuelGesturePrimitiveV1] {
        &self.plan.primitives
    }

    pub fn action_family_v1(&self) -> MtgoDuelActionFamilyV1 {
        self.action_family
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

pub fn validate_player_visible_duel_gesture_plan_v1(
    plan: MtgoPlayerVisibleDuelGesturePlanV1,
) -> Result<CheckedUntrustedMtgoPlayerVisibleDuelGesturePlanV1, MtgoContractErrorV1> {
    if plan.schema_version != MTGO_PLAYER_VISIBLE_DUEL_GESTURE_PLAN_SCHEMA_V1 {
        return Err(error_v1(
            "player_visible_duel_gesture_schema_mismatch",
            plan.schema_version.to_string(),
        ));
    }
    if action_actor_v1(&plan.selected_action) != MtgoPlayerRelativeRoleV1::SeatedPlayer {
        return Err(error_v1(
            "player_visible_duel_gesture_actor_invalid",
            "only the seated player's visible legal action may produce a gesture",
        ));
    }
    if !plan.primitive_set_complete
        || plan.primitives.is_empty()
        || plan.primitives.len() > MAX_PLAYER_VISIBLE_DUEL_GESTURE_PRIMITIVES_V1
    {
        return Err(error_v1(
            "player_visible_duel_gesture_primitive_set_invalid",
            plan.primitives.len().to_string(),
        ));
    }
    validate_player_visible_duel_gesture_shape_v1(&plan.selected_action, &plan.primitives)?;
    let action_family = player_visible_duel_action_family_v1(&plan.selected_action);
    Ok(CheckedUntrustedMtgoPlayerVisibleDuelGesturePlanV1 {
        plan,
        action_family,
    })
}

pub fn player_visible_duel_action_family_v1(
    action: &MtgoPlayerVisibleDuelActionV1,
) -> MtgoDuelActionFamilyV1 {
    match action {
        MtgoPlayerVisibleDuelActionV1::Pass { .. } => MtgoDuelActionFamilyV1::PriorityPass,
        MtgoPlayerVisibleDuelActionV1::PlayLand { .. } => MtgoDuelActionFamilyV1::PlayLand,
        MtgoPlayerVisibleDuelActionV1::CastSpell { .. }
        | MtgoPlayerVisibleDuelActionV1::PlotSpell { .. } => {
            MtgoDuelActionFamilyV1::CastOrPlotSpell
        }
        MtgoPlayerVisibleDuelActionV1::ActivateManaAbility { .. } => {
            MtgoDuelActionFamilyV1::ManaAbility
        }
        MtgoPlayerVisibleDuelActionV1::ActivateAbility { .. } => {
            MtgoDuelActionFamilyV1::NonManaAbility
        }
        MtgoPlayerVisibleDuelActionV1::ChooseTarget { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseCostTarget { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectTarget { .. }
        | MtgoPlayerVisibleDuelActionV1::FinishTargetSelection { .. } => {
            MtgoDuelActionFamilyV1::TargetChoice
        }
        MtgoPlayerVisibleDuelActionV1::ChooseCastMode { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseKicker { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseSpellMode { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseOptionalCostUse { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseOptionalCostWhich { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseSpellCopyPayment { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseSpellCopyRetarget { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseMadnessCast { .. } => {
            MtgoDuelActionFamilyV1::CostOrModeChoice
        }
        MtgoPlayerVisibleDuelActionV1::ChooseEffectOption { .. }
        | MtgoPlayerVisibleDuelActionV1::FinishEffectSelection { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectColor { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectNumber { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectBoolean { .. } => {
            MtgoDuelActionFamilyV1::EffectChoice
        }
        MtgoPlayerVisibleDuelActionV1::Discard { .. } => MtgoDuelActionFamilyV1::Discard,
        MtgoPlayerVisibleDuelActionV1::DeclareAttackers { .. }
        | MtgoPlayerVisibleDuelActionV1::DeclareBlockersForAttacker { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseAttackerInclusion { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseBlockerInclusion { .. } => {
            MtgoDuelActionFamilyV1::CombatChoice
        }
        MtgoPlayerVisibleDuelActionV1::OrderTriggers { .. } => {
            MtgoDuelActionFamilyV1::TriggerOrdering
        }
    }
}

pub fn required_player_visible_duel_gesture_target_roles_v1(
    primitive: &MtgoPlayerVisibleDuelGesturePrimitiveV1,
) -> Vec<MtgoPlayerVisibleDuelGestureTargetRoleV1> {
    match primitive {
        MtgoPlayerVisibleDuelGesturePrimitiveV1::ActivatePrimary { .. } => {
            vec![MtgoPlayerVisibleDuelGestureTargetRoleV1::PrimarySemanticControl]
        }
        MtgoPlayerVisibleDuelGesturePrimitiveV1::ActivateSemanticMenuChoice { .. } => {
            vec![MtgoPlayerVisibleDuelGestureTargetRoleV1::SemanticMenuChoice]
        }
        MtgoPlayerVisibleDuelGesturePrimitiveV1::DragPrimaryToCalibratedPlayArea => vec![
            MtgoPlayerVisibleDuelGestureTargetRoleV1::PrimarySemanticControl,
            MtgoPlayerVisibleDuelGestureTargetRoleV1::CalibratedPlayArea,
        ],
        MtgoPlayerVisibleDuelGesturePrimitiveV1::SelectVisibleObject { object } => {
            vec![MtgoPlayerVisibleDuelGestureTargetRoleV1::VisibleObject { object: *object }]
        }
        MtgoPlayerVisibleDuelGesturePrimitiveV1::DragVisibleObjectToOrderSlot {
            object,
            slot_index,
        } => vec![
            MtgoPlayerVisibleDuelGestureTargetRoleV1::VisibleObject { object: *object },
            MtgoPlayerVisibleDuelGestureTargetRoleV1::OrderSlot {
                slot_index: *slot_index,
            },
        ],
        MtgoPlayerVisibleDuelGesturePrimitiveV1::Submit => {
            vec![MtgoPlayerVisibleDuelGestureTargetRoleV1::SubmitControl]
        }
    }
}

fn validate_player_visible_duel_gesture_shape_v1(
    action: &MtgoPlayerVisibleDuelActionV1,
    primitives: &[MtgoPlayerVisibleDuelGesturePrimitiveV1],
) -> Result<(), MtgoContractErrorV1> {
    match action {
        MtgoPlayerVisibleDuelActionV1::Pass { .. } => {
            require_direct_primary_v1(primitives, &[MtgoDuelPrimaryActivationV1::SingleLeftClick])
        }
        MtgoPlayerVisibleDuelActionV1::PlayLand { .. }
        | MtgoPlayerVisibleDuelActionV1::CastSpell { .. }
        | MtgoPlayerVisibleDuelActionV1::PlotSpell { .. } => {
            if require_direct_primary_v1(
                primitives,
                &[
                    MtgoDuelPrimaryActivationV1::SingleLeftClick,
                    MtgoDuelPrimaryActivationV1::DoubleLeftClick,
                ],
            )
            .is_ok()
                || matches!(
                    primitives,
                    [MtgoPlayerVisibleDuelGesturePrimitiveV1::DragPrimaryToCalibratedPlayArea]
                )
            {
                Ok(())
            } else {
                Err(shape_error_v1("play or cast gesture shape is invalid"))
            }
        }
        MtgoPlayerVisibleDuelActionV1::ActivateManaAbility { .. }
        | MtgoPlayerVisibleDuelActionV1::ActivateAbility { .. } => {
            if require_direct_primary_v1(
                primitives,
                &[
                    MtgoDuelPrimaryActivationV1::SingleLeftClick,
                    MtgoDuelPrimaryActivationV1::DoubleLeftClick,
                ],
            )
            .is_ok()
                || is_context_menu_v1(primitives)
            {
                Ok(())
            } else {
                Err(shape_error_v1("ability gesture shape is invalid"))
            }
        }
        MtgoPlayerVisibleDuelActionV1::Discard { cards, .. }
        | MtgoPlayerVisibleDuelActionV1::DeclareAttackers {
            attackers: cards, ..
        }
        | MtgoPlayerVisibleDuelActionV1::DeclareBlockersForAttacker {
            blockers: cards, ..
        } => require_object_selection_then_submit_v1(primitives, cards),
        MtgoPlayerVisibleDuelActionV1::OrderTriggers {
            pending_sources,
            ordered_sources,
            ..
        } => require_trigger_order_v1(primitives, pending_sources, ordered_sources),
        _ => require_direct_primary_v1(primitives, &[MtgoDuelPrimaryActivationV1::SingleLeftClick]),
    }
}

fn require_direct_primary_v1(
    primitives: &[MtgoPlayerVisibleDuelGesturePrimitiveV1],
    allowed: &[MtgoDuelPrimaryActivationV1],
) -> Result<(), MtgoContractErrorV1> {
    match primitives {
        [MtgoPlayerVisibleDuelGesturePrimitiveV1::ActivatePrimary { activation }]
            if allowed.contains(activation) =>
        {
            Ok(())
        }
        _ => Err(shape_error_v1("direct primary gesture shape is invalid")),
    }
}

fn is_context_menu_v1(primitives: &[MtgoPlayerVisibleDuelGesturePrimitiveV1]) -> bool {
    matches!(
        primitives,
        [
            MtgoPlayerVisibleDuelGesturePrimitiveV1::ActivatePrimary {
                activation: MtgoDuelPrimaryActivationV1::RightClick
            },
            MtgoPlayerVisibleDuelGesturePrimitiveV1::ActivateSemanticMenuChoice {
                activation: MtgoDuelPrimaryActivationV1::SingleLeftClick
            }
        ]
    )
}

fn require_object_selection_then_submit_v1(
    primitives: &[MtgoPlayerVisibleDuelGesturePrimitiveV1],
    expected_objects: &[MtgoPlayerVisibleObjectRefV1],
) -> Result<(), MtgoContractErrorV1> {
    require_unique_visible_objects_v1(expected_objects)?;
    if primitives.len() != expected_objects.len() + 1
        || !matches!(
            primitives.last(),
            Some(MtgoPlayerVisibleDuelGesturePrimitiveV1::Submit)
        )
    {
        return Err(shape_error_v1(
            "compound gesture must select each visible object then submit",
        ));
    }
    for (primitive, expected) in primitives.iter().zip(expected_objects) {
        match primitive {
            MtgoPlayerVisibleDuelGesturePrimitiveV1::SelectVisibleObject { object }
                if object == expected => {}
            _ => {
                return Err(shape_error_v1(
                    "visible object selection order differs from the selected action",
                ))
            }
        }
    }
    Ok(())
}

fn require_trigger_order_v1(
    primitives: &[MtgoPlayerVisibleDuelGesturePrimitiveV1],
    pending_sources: &[MtgoPlayerVisibleObjectRefV1],
    ordered_sources: &[MtgoPlayerVisibleObjectRefV1],
) -> Result<(), MtgoContractErrorV1> {
    require_unique_visible_objects_v1(pending_sources)?;
    require_unique_visible_objects_v1(ordered_sources)?;
    let pending = pending_sources.iter().copied().collect::<HashSet<_>>();
    let ordered = ordered_sources.iter().copied().collect::<HashSet<_>>();
    if pending_sources.is_empty()
        || pending_sources.len() != ordered_sources.len()
        || pending != ordered
        || primitives.len() != ordered_sources.len() + 1
        || !matches!(
            primitives.last(),
            Some(MtgoPlayerVisibleDuelGesturePrimitiveV1::Submit)
        )
    {
        return Err(shape_error_v1(
            "trigger-order gesture cardinality is invalid",
        ));
    }
    let click_shape = primitives[..ordered_sources.len()]
        .iter()
        .zip(ordered_sources)
        .all(|(primitive, expected)| {
            matches!(
                primitive,
                MtgoPlayerVisibleDuelGesturePrimitiveV1::SelectVisibleObject { object }
                    if object == expected
            )
        });
    let drag_shape = primitives[..ordered_sources.len()]
        .iter()
        .zip(ordered_sources)
        .enumerate()
        .all(|(slot_index, (primitive, expected))| {
            matches!(
                primitive,
                MtgoPlayerVisibleDuelGesturePrimitiveV1::DragVisibleObjectToOrderSlot {
                    object,
                    slot_index: actual_slot,
                } if object == expected && usize::from(*actual_slot) == slot_index
            )
        });
    if click_shape || drag_shape {
        Ok(())
    } else {
        Err(shape_error_v1(
            "trigger gesture does not implement the selected visible order",
        ))
    }
}

fn require_unique_visible_objects_v1(
    objects: &[MtgoPlayerVisibleObjectRefV1],
) -> Result<(), MtgoContractErrorV1> {
    if objects.iter().copied().collect::<HashSet<_>>().len() != objects.len() {
        return Err(shape_error_v1("visible object list contains duplicates"));
    }
    Ok(())
}

fn action_actor_v1(action: &MtgoPlayerVisibleDuelActionV1) -> MtgoPlayerRelativeRoleV1 {
    match action {
        MtgoPlayerVisibleDuelActionV1::Pass { actor }
        | MtgoPlayerVisibleDuelActionV1::PlayLand { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::CastSpell { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ActivateManaAbility { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ActivateAbility { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::PlotSpell { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseTarget { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseCostTarget { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseCastMode { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseKicker { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseSpellMode { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectOption { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectTarget { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::FinishEffectSelection { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectColor { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectNumber { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectBoolean { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::FinishTargetSelection { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseOptionalCostUse { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseOptionalCostWhich { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseSpellCopyPayment { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseSpellCopyRetarget { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseMadnessCast { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::Discard { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::DeclareAttackers { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::DeclareBlockersForAttacker { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseAttackerInclusion { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseBlockerInclusion { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::OrderTriggers { actor, .. } => *actor,
    }
}

fn shape_error_v1(detail: impl Into<String>) -> MtgoContractErrorV1 {
    error_v1("player_visible_duel_gesture_shape_invalid", detail)
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object_v1(visible_ordinal: u32) -> MtgoPlayerVisibleObjectRefV1 {
        MtgoPlayerVisibleObjectRefV1 { visible_ordinal }
    }

    fn plan_v1(
        selected_action: MtgoPlayerVisibleDuelActionV1,
        primitives: Vec<MtgoPlayerVisibleDuelGesturePrimitiveV1>,
    ) -> MtgoPlayerVisibleDuelGesturePlanV1 {
        MtgoPlayerVisibleDuelGesturePlanV1 {
            schema_version: MTGO_PLAYER_VISIBLE_DUEL_GESTURE_PLAN_SCHEMA_V1,
            selected_action,
            primitive_set_complete: true,
            primitives,
        }
    }

    #[test]
    fn direct_and_context_menu_shapes_validate_without_authority() {
        let pass = validate_player_visible_duel_gesture_plan_v1(plan_v1(
            MtgoPlayerVisibleDuelActionV1::Pass {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            },
            vec![MtgoPlayerVisibleDuelGesturePrimitiveV1::ActivatePrimary {
                activation: MtgoDuelPrimaryActivationV1::SingleLeftClick,
            }],
        ))
        .unwrap();
        assert_eq!(
            pass.action_family_v1(),
            MtgoDuelActionFamilyV1::PriorityPass
        );
        assert!(!pass.safe_for_live_input_v1());
        assert!(!pass.permits_event_entry_v1());
        assert!(!pass.permits_spending_v1());

        validate_player_visible_duel_gesture_plan_v1(plan_v1(
            MtgoPlayerVisibleDuelActionV1::ActivateAbility {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                source: object_v1(3),
                visible_choice_ordinal: 1,
            },
            vec![
                MtgoPlayerVisibleDuelGesturePrimitiveV1::ActivatePrimary {
                    activation: MtgoDuelPrimaryActivationV1::RightClick,
                },
                MtgoPlayerVisibleDuelGesturePrimitiveV1::ActivateSemanticMenuChoice {
                    activation: MtgoDuelPrimaryActivationV1::SingleLeftClick,
                },
            ],
        ))
        .unwrap();
    }

    #[test]
    fn compound_selection_uses_exact_visible_ordinals() {
        let selected = vec![object_v1(5), object_v1(2)];
        validate_player_visible_duel_gesture_plan_v1(plan_v1(
            MtgoPlayerVisibleDuelActionV1::Discard {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                cards: selected.clone(),
            },
            vec![
                MtgoPlayerVisibleDuelGesturePrimitiveV1::SelectVisibleObject {
                    object: selected[0],
                },
                MtgoPlayerVisibleDuelGesturePrimitiveV1::SelectVisibleObject {
                    object: selected[1],
                },
                MtgoPlayerVisibleDuelGesturePrimitiveV1::Submit,
            ],
        ))
        .unwrap();

        assert!(validate_player_visible_duel_gesture_plan_v1(plan_v1(
            MtgoPlayerVisibleDuelActionV1::Discard {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                cards: selected.clone(),
            },
            vec![
                MtgoPlayerVisibleDuelGesturePrimitiveV1::SelectVisibleObject {
                    object: selected[1],
                },
                MtgoPlayerVisibleDuelGesturePrimitiveV1::SelectVisibleObject {
                    object: selected[0],
                },
                MtgoPlayerVisibleDuelGesturePrimitiveV1::Submit,
            ],
        ))
        .is_err());
    }

    #[test]
    fn trigger_order_accepts_exact_visible_permutation_and_slots() {
        let pending = vec![object_v1(1), object_v1(2), object_v1(3)];
        let ordered = vec![object_v1(3), object_v1(1), object_v1(2)];
        let checked = validate_player_visible_duel_gesture_plan_v1(plan_v1(
            MtgoPlayerVisibleDuelActionV1::OrderTriggers {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                pending_sources: pending,
                ordered_sources: ordered.clone(),
            },
            vec![
                MtgoPlayerVisibleDuelGesturePrimitiveV1::DragVisibleObjectToOrderSlot {
                    object: ordered[0],
                    slot_index: 0,
                },
                MtgoPlayerVisibleDuelGesturePrimitiveV1::DragVisibleObjectToOrderSlot {
                    object: ordered[1],
                    slot_index: 1,
                },
                MtgoPlayerVisibleDuelGesturePrimitiveV1::DragVisibleObjectToOrderSlot {
                    object: ordered[2],
                    slot_index: 2,
                },
                MtgoPlayerVisibleDuelGesturePrimitiveV1::Submit,
            ],
        ))
        .unwrap();
        assert_eq!(
            checked.action_family_v1(),
            MtgoDuelActionFamilyV1::TriggerOrdering
        );
    }

    #[test]
    fn opponent_incomplete_duplicate_and_malformed_plans_reject() {
        let mut opponent = plan_v1(
            MtgoPlayerVisibleDuelActionV1::Pass {
                actor: MtgoPlayerRelativeRoleV1::Opponent,
            },
            vec![MtgoPlayerVisibleDuelGesturePrimitiveV1::ActivatePrimary {
                activation: MtgoDuelPrimaryActivationV1::SingleLeftClick,
            }],
        );
        assert!(validate_player_visible_duel_gesture_plan_v1(opponent.clone()).is_err());
        opponent.selected_action = MtgoPlayerVisibleDuelActionV1::Pass {
            actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
        };
        opponent.primitive_set_complete = false;
        assert!(validate_player_visible_duel_gesture_plan_v1(opponent).is_err());

        let duplicate = object_v1(4);
        assert!(validate_player_visible_duel_gesture_plan_v1(plan_v1(
            MtgoPlayerVisibleDuelActionV1::DeclareAttackers {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                attackers: vec![duplicate, duplicate],
            },
            vec![
                MtgoPlayerVisibleDuelGesturePrimitiveV1::SelectVisibleObject { object: duplicate },
                MtgoPlayerVisibleDuelGesturePrimitiveV1::SelectVisibleObject { object: duplicate },
                MtgoPlayerVisibleDuelGesturePrimitiveV1::Submit,
            ],
        ))
        .is_err());
    }

    #[test]
    fn serialized_plan_contains_no_kernel_or_transport_identity() {
        let plan = plan_v1(
            MtgoPlayerVisibleDuelActionV1::PlayLand {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                source: object_v1(7),
            },
            vec![MtgoPlayerVisibleDuelGesturePrimitiveV1::ActivatePrimary {
                activation: MtgoDuelPrimaryActivationV1::DoubleLeftClick,
            }],
        );
        let json = serde_json::to_string(&plan).unwrap();
        for forbidden in [
            "arena_id",
            "card_db_id",
            "zone_change_count",
            "frame_id",
            "frame_sequence",
            "control_id",
            "decision_commitment",
            "source_manifest",
            "profile_bound",
            "rect_client_px",
            "authorization",
        ] {
            assert!(
                !json.contains(forbidden),
                "forbidden field leaked: {forbidden}"
            );
        }
    }

    #[test]
    fn target_roles_preserve_only_visible_ordinal_and_slot() {
        let object = object_v1(9);
        assert_eq!(
            required_player_visible_duel_gesture_target_roles_v1(
                &MtgoPlayerVisibleDuelGesturePrimitiveV1::DragVisibleObjectToOrderSlot {
                    object,
                    slot_index: 2,
                },
            ),
            vec![
                MtgoPlayerVisibleDuelGestureTargetRoleV1::VisibleObject { object },
                MtgoPlayerVisibleDuelGestureTargetRoleV1::OrderSlot { slot_index: 2 },
            ]
        );
        let json = serde_json::to_string(&required_player_visible_duel_gesture_target_roles_v1(
            &MtgoPlayerVisibleDuelGesturePrimitiveV1::SelectVisibleObject { object },
        ))
        .unwrap();
        for forbidden in [
            "arena_id",
            "card_db_id",
            "zone_change_count",
            "frame_id",
            "evidence_id",
            "rect_client_px",
        ] {
            assert!(
                !json.contains(forbidden),
                "forbidden role field leaked: {forbidden}"
            );
        }
    }
}
